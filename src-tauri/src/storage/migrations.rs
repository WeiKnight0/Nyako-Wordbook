use crate::error::AppError;
use rusqlite::{backup::Backup, Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const APPLICATION_ID: i64 = 0x4e594133;
pub(super) const MIGRATIONS: &[&str] = &[
    include_str!("migrations/003_wordbook.sql"),
    include_str!("migrations/002_search.sql"),
    include_str!("migrations/004_verb_transitivity.sql"),
];

fn timestamp(time: SystemTime) -> Result<i64, AppError> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .map_err(|_| AppError::validation("系统时间无效"))?
            .as_millis(),
    )
    .map_err(|_| AppError::validation("系统时间超出支持范围"))
}

fn now_ms() -> Result<i64, AppError> {
    timestamp(SystemTime::now())
}

fn checksum(sql: &str) -> String {
    Sha256::digest(sql.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn invalid(message: &str) -> AppError {
    AppError::new("SCHEMA_INVALID", message)
}

fn verify_integrity(conn: &Connection) -> Result<(), AppError> {
    let integrity = crate::timing::Timing::start("storage/integrity");
    if conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err(AppError::new(
            "DB_CORRUPT",
            "数据库完整性检查失败；原数据已保留",
        ));
    }
    drop(integrity);
    let _foreign_keys = crate::timing::Timing::start("storage/foreign_keys");
    if conn
        .prepare("PRAGMA foreign_key_check")?
        .query([])?
        .next()?
        .is_some()
    {
        return Err(invalid("数据库存在无效关联，已停止写入"));
    }
    Ok(())
}

fn verify_version(conn: &Connection, scripts: &[&str]) -> Result<usize, AppError> {
    let metadata = crate::timing::Timing::start("storage/version_metadata");
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > scripts.len() as i64 {
        return Err(AppError::new(
            "SCHEMA_TOO_NEW",
            "数据库来自更高版本，请升级应用",
        ));
    }
    if conn.query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))? != APPLICATION_ID
        || version <= 0
    {
        return Err(invalid(
            "不是已识别的 NyakoWordbook 业务数据库，未修改原文件",
        ));
    }
    let mut stmt = conn
        .prepare("SELECT version,checksum FROM schema_migrations ORDER BY version")
        .map_err(|_| invalid("缺少数据库迁移记录"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() != version as usize {
        return Err(invalid("数据库版本与迁移记录不一致"));
    }
    if rows
        .iter()
        .enumerate()
        .any(|(i, (number, hash))| *number != i as i64 + 1 || *hash != checksum(scripts[i]))
    {
        return Err(invalid("迁移版本不连续或脚本校验和不一致"));
    }
    drop(metadata);
    verify_integrity(conn)?;
    Ok(version as usize)
}

fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;")?;
    Ok(())
}

fn write_snapshot(conn: &Connection, temporary: &Path, final_path: &Path) -> Result<(), AppError> {
    let mut target = Connection::open(temporary)?;
    Backup::new(conn, &mut target)?.run_to_completion(128, Duration::from_millis(10), None)?;
    verify_integrity(&target)?;
    drop(target);
    std::fs::OpenOptions::new()
        .write(true)
        .open(temporary)?
        .sync_all()?;
    std::fs::rename(temporary, final_path)?;
    Ok(())
}

fn snapshot(conn: &Connection, backups: &Path, version: usize) -> Result<(), AppError> {
    std::fs::create_dir_all(backups)?;
    let name = format!("pre-migration-v{version}-{}-{}", now_ms()?, Uuid::new_v4());
    let temporary = backups.join(format!("{name}.tmp"));
    let result = write_snapshot(conn, &temporary, &backups.join(format!("{name}.sqlite3")));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|_| AppError::new("BACKUP_FAILED", "迁移前备份失败，尚未执行迁移"))
}

fn migrate(conn: &mut Connection, scripts: &[&str], version: usize) -> Result<(), AppError> {
    if version == scripts.len() {
        return Ok(());
    }
    let tx = conn.transaction()?;
    for (i, sql) in scripts.iter().enumerate().skip(version) {
        tx.execute_batch(sql).map_err(|_| {
            AppError::new(
                "MIGRATION_FAILED",
                "数据库迁移失败，变更已回滚，原数据及迁移前备份保留",
            )
        })?;
        tx.execute(
            "INSERT INTO schema_migrations VALUES(?1,?2,?3)",
            rusqlite::params![i as i64 + 1, checksum(sql), now_ms()?],
        )?;
    }
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", scripts.len() as i64)?;
    tx.execute("INSERT OR IGNORE INTO scheduler_parameters VALUES('fsrs6-default','FSRS','6.6.2',?1,0.90,?2)",
        rusqlite::params![serde_json::to_string(&fsrs::DEFAULT_PARAMETERS)
            .map_err(|_| invalid("默认调度参数不可序列化"))?, now_ms()?])?;
    verify_integrity(&tx)?;
    tx.commit()?;
    Ok(())
}

fn prepare_database(
    working: &Path,
    backups: &Path,
    scripts: &[&str],
    version: usize,
) -> Result<(), AppError> {
    let _prepare = crate::timing::Timing::start("storage/prepare_connection");
    let mut conn = Connection::open(working)?;
    configure(&conn)?;
    if version > 0 && version < scripts.len() {
        snapshot(&conn, backups, version)?;
    }
    if version < scripts.len() {
        migrate(&mut conn, scripts, version)?;
        verify_version(&conn, scripts)?;
    }
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    Ok(())
}

fn publish_database(
    path: &Path,
    working: &Path,
    backups: &Path,
    scripts: &[&str],
    version: usize,
    existing: bool,
) -> Result<Connection, AppError> {
    prepare_database(working, backups, scripts, version)?;
    if !existing {
        std::fs::rename(working, path)?;
    }
    let _reopen = crate::timing::Timing::start("storage/reopen_configure");
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

/// @description 校验正式库并在备份保护下升级到当前版本。
/// @param path 正式库路径；backups 升级前快照目录。
/// @throws 返回版本、完整性、备份、迁移或文件访问错误。
pub(super) fn open_database(path: &Path, backups: &Path) -> Result<Connection, AppError> {
    open_with_scripts(path, backups, MIGRATIONS)
}

/// @description 使用指定迁移序列初始化或升级数据库。
/// @param path 数据库路径；backups 快照目录；scripts 按版本排列的冻结脚本。
/// @throws 返回数据库校验、迁移或文件访问错误。
pub(super) fn open_with_scripts(
    path: &Path,
    backups: &Path,
    scripts: &[&str],
) -> Result<Connection, AppError> {
    let existing = path.exists();
    let version = if existing {
        let opened = crate::timing::Timing::start("storage/readonly_open");
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        drop(opened);
        verify_version(&conn, scripts)?
    } else {
        0
    };
    let working = if existing {
        path.to_path_buf()
    } else {
        path.with_file_name(format!("initialize-{}.tmp", Uuid::new_v4()))
    };
    let result = publish_database(path, &working, backups, scripts, version, existing);
    if result.is_err() && !existing {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", working.display()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_time_is_rejected_instead_of_becoming_epoch() {
        assert_eq!(
            timestamp(UNIX_EPOCH - Duration::from_secs(1))
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_millis(1234)).unwrap(),
            1234
        );
    }
}
