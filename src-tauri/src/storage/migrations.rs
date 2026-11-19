use crate::error::AppError;
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::path::Path;

const APPLICATION_ID: i64 = 0x4e595458;
pub(super) const MIGRATIONS: &[&str] = &[include_str!("migrations/001_wordbook.sql")];

fn checksum() -> String {
    Sha256::digest(MIGRATIONS[0].as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate(conn: &Connection) -> Result<(), AppError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > 1 {
        return Err(AppError::new("SCHEMA_TOO_NEW", "数据库版本不受支持"));
    }
    if version != 1
        || conn.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))?
            != APPLICATION_ID
    {
        return Err(AppError::new(
            "SCHEMA_INVALID",
            "不是当前文本模型数据库，请使用新建数据库",
        ));
    }
    let records = conn
        .prepare("SELECT version,checksum FROM schema_migrations")?
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if records != vec![(1, checksum())] {
        return Err(AppError::new("SCHEMA_INVALID", "数据库建库记录不一致"));
    }
    if conn.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))? != "ok" {
        return Err(AppError::new("DB_CORRUPT", "数据库快速检查失败"));
    }
    Ok(())
}

fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;")?;
    Ok(())
}

/// @description 创建或打开当前文本模型数据库，不转换旧数据。
/// @param path 正式数据库路径。
/// @throws 返回格式、完整性、建库或文件访问错误。
pub(super) fn open_database(path: &Path) -> Result<Connection, AppError> {
    if path.exists() {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        validate(&conn)?;
        drop(conn);
        let conn = Connection::open(path)?;
        configure(&conn)?;
        return Ok(conn);
    }
    let temporary = path.with_file_name(format!("initialize-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut conn = Connection::open(&temporary)?;
        configure(&conn)?;
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATIONS[0])?;
        tx.execute(
            "INSERT INTO schema_migrations VALUES(1,?1,?2)",
            rusqlite::params![checksum(), super::libraries::now()?],
        )?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.execute(
            "INSERT INTO scheduler_parameters VALUES('fsrs6-default','FSRS','6.6.2',?1,0.90,?2)",
            rusqlite::params![
                serde_json::to_string(&fsrs::DEFAULT_PARAMETERS)
                    .map_err(|error| AppError::validation(&error.to_string()))?,
                super::libraries::now()?
            ],
        )?;
        tx.commit()?;
        validate(&conn)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        drop(conn);
        std::fs::rename(&temporary, path)?;
        let conn = Connection::open(path)?;
        configure(&conn)?;
        Ok(conn)
    })();
    if result.is_err() {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", temporary.display()));
        }
    }
    result
}
