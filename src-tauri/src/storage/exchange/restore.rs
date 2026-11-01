use super::{
    backup, encode,
    format::{id, json_error},
};
use crate::{error::AppError, storage::migrations};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    task_id: String,
    old_id: String,
}

/// @description 为下次启动安排已校验的完整恢复，先保留当前安全备份。
/// @param conn 正式库连接；root 数据根；task 已校验的恢复任务。
/// @throws 安全备份失败或候选无效时不安排覆盖。
pub(super) fn schedule(
    conn: &Connection,
    root: &Path,
    task: &str,
) -> Result<serde_json::Value, AppError> {
    crate::storage::validation::id(task)?;
    let candidate = root.join("staging").join(task).join("restore");
    backup::verify_candidate(&candidate)?;
    let source = Connection::open_with_flags(
        candidate.join("data/main.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    if source.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err(AppError::new(
            "INVALID_BACKUP",
            "恢复候选数据库完整性检查失败",
        ));
    }
    drop(source);
    let journal = root.join("restore-journal.json");
    if journal.exists() {
        return Err(AppError::new("MAINTENANCE", "已有恢复正在等待重启"));
    }
    std::fs::create_dir_all(root.join("backups"))?;
    let safety = root
        .join("backups")
        .join(format!("before-restore-{}.nyakobak", id()));
    backup::create(conn, root, &safety)?;
    let value = Journal {
        task_id: task.into(),
        old_id: id(),
    };
    let temporary = root.join(format!(".restore-journal-{}.tmp", id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(encode(&value)?.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::hard_link(&temporary, &journal)?;
    std::fs::remove_file(temporary)?;
    Ok(serde_json::json!({"restart_required":true,"safety_backup":safety}))
}

fn mark(path: &Path) -> Result<(), AppError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .sync_all()?;
    Ok(())
}

/// @description 在业务数据库打开之前完成目录恢复或回退，避免创建空库。
/// @param root 应用数据根目录。
/// @throws 回退或验证失败时阻止正常启动并保留所有恢复资产。
pub(crate) fn recover(root: &Path) -> Result<(), AppError> {
    let journal = root.join("restore-journal.json");
    if !journal.exists() {
        return Ok(());
    }
    let value: Journal = serde_json::from_slice(&std::fs::read(&journal)?).map_err(json_error)?;
    crate::storage::validation::id(&value.task_id)?;
    crate::storage::validation::id(&value.old_id)?;
    let candidate = root.join("staging").join(&value.task_id).join("restore");
    let old = root
        .join("backups")
        .join(format!("restore-old-{}", value.old_id));
    std::fs::create_dir_all(&old)?;
    let started = old.join("started");
    let switched = old.join("switched");
    let verified = old.join("verified");
    if verified.exists() {
        std::fs::remove_file(journal)?;
        return Ok(());
    }
    if old.join("rolling-back").exists() || started.exists() && !switched.exists() {
        rollback(root, &old)?;
        std::fs::remove_file(journal)?;
        return Ok(());
    }
    if !started.exists() {
        if !candidate.join("data/main.sqlite3").is_file() {
            return Err(AppError::new(
                "RESTORE_FAILED",
                "恢复候选不存在，原数据已保留",
            ));
        }
        backup::verify_candidate(&candidate)?;
        std::fs::create_dir_all(candidate.join("media"))?;
        mark(&started)?;
        let switch = (|| {
            for folder in ["data", "media"] {
                if root.join(folder).exists() {
                    std::fs::rename(root.join(folder), old.join(folder))?;
                } else {
                    mark(&old.join(format!("{folder}-was-absent")))?;
                }
                mark(&old.join(format!("{folder}-old-moved")))?;
                std::fs::rename(candidate.join(folder), root.join(folder))?;
            }
            mark(&switched)
        })();
        if let Err(error) = switch {
            rollback(root, &old)?;
            std::fs::remove_file(journal)?;
            return Err(error);
        }
    }
    let validation = if root.join("data/main.sqlite3").is_file() {
        migrations::open_database(&root.join("data/main.sqlite3"), &root.join("backups"))
    } else {
        Err(AppError::new(
            "RESTORE_FAILED",
            "恢复后数据库缺失，拒绝创建空库",
        ))
    };
    match validation {
        Ok(conn) => {
            drop(conn);
            mark(&verified)?;
            std::fs::remove_file(journal)?;
            Ok(())
        }
        Err(error) => {
            rollback(root, &old)?;
            std::fs::remove_file(journal)?;
            Err(error)
        }
    }
}

fn rollback(root: &Path, old: &Path) -> Result<(), AppError> {
    if !old.join("rolling-back").exists() {
        mark(&old.join("rolling-back"))?;
    }
    for folder in ["media", "data"] {
        let previous = old.join(folder);
        if !previous.exists() {
            if old.join(format!("{folder}-was-absent")).exists() && root.join(folder).exists() {
                std::fs::rename(
                    root.join(folder),
                    old.join(format!("failed-{folder}-{}", id())),
                )?;
            }
            continue;
        }
        if root.join(folder).exists() {
            std::fs::rename(
                root.join(folder),
                old.join(format!("failed-{folder}-{}", id())),
            )?;
        }
        std::fs::rename(previous, root.join(folder))?;
    }
    if !root.join("data/main.sqlite3").is_file() {
        return Err(AppError::new(
            "RESTORE_ROLLBACK_FAILED",
            "恢复回退未找到完整旧数据库，已停止启动",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_resumes_after_old_data_returned_and_removes_previously_absent_media() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        std::fs::create_dir_all(root.path().join("data")).unwrap();
        std::fs::create_dir_all(root.path().join("media")).unwrap();
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(root.path().join("data/main.sqlite3"), b"old-database").unwrap();
        std::fs::write(root.path().join("media/new"), b"new-media").unwrap();
        mark(&old.join("rolling-back")).unwrap();
        mark(&old.join("media-was-absent")).unwrap();
        rollback(root.path(), &old).unwrap();
        rollback(root.path(), &old).unwrap();
        assert!(!root.path().join("media").exists());
        assert_eq!(
            std::fs::read(root.path().join("data/main.sqlite3")).unwrap(),
            b"old-database"
        );
    }

    #[test]
    fn every_interrupted_directory_switch_rolls_back_to_complete_old_data() {
        for phase in 0..5 {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(root.path().join("data")).unwrap();
            std::fs::create_dir_all(root.path().join("media")).unwrap();
            std::fs::write(root.path().join("data/main.sqlite3"), b"old-database").unwrap();
            std::fs::write(root.path().join("media/old"), b"old-media").unwrap();
            let value = Journal {
                task_id: id(),
                old_id: id(),
            };
            let candidate = root
                .path()
                .join("staging")
                .join(&value.task_id)
                .join("restore");
            let old = root
                .path()
                .join("backups")
                .join(format!("restore-old-{}", value.old_id));
            std::fs::create_dir_all(candidate.join("data")).unwrap();
            std::fs::create_dir_all(candidate.join("media")).unwrap();
            std::fs::create_dir_all(&old).unwrap();
            std::fs::write(candidate.join("data/main.sqlite3"), b"new-database").unwrap();
            std::fs::write(candidate.join("media/new"), b"new-media").unwrap();
            std::fs::write(
                root.path().join("restore-journal.json"),
                encode(&value).unwrap(),
            )
            .unwrap();
            mark(&old.join("started")).unwrap();
            if phase >= 1 {
                std::fs::rename(root.path().join("data"), old.join("data")).unwrap();
            }
            if phase >= 2 {
                std::fs::rename(candidate.join("data"), root.path().join("data")).unwrap();
            }
            if phase >= 3 {
                std::fs::rename(root.path().join("media"), old.join("media")).unwrap();
            }
            if phase >= 4 {
                std::fs::rename(candidate.join("media"), root.path().join("media")).unwrap();
            }
            recover(root.path()).unwrap();
            assert_eq!(
                std::fs::read(root.path().join("data/main.sqlite3")).unwrap(),
                b"old-database"
            );
            assert_eq!(
                std::fs::read(root.path().join("media/old")).unwrap(),
                b"old-media"
            );
            assert!(!root.path().join("restore-journal.json").exists());
        }
    }
}
