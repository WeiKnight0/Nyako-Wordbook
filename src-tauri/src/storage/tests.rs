use super::*;
use migrations::{open_database, MIGRATIONS};

fn word(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO words(id,kanji,kana,meaning,created_at,updated_at) VALUES(?1,'働く','はたらく','工作',1,1)",
        [id],
    )
    .unwrap();
}

#[test]
fn schema_constraints_and_aggregate_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    let mut conn = open_database(&path).unwrap();
    word(&conn, "a");
    word(&conn, "b");
    assert!(conn
        .execute(
            "INSERT INTO words(id,kanji,kana,meaning,created_at,updated_at) VALUES('empty','','ねこ','猫',1,1)",
            []
        )
        .is_err());
    conn.execute("INSERT INTO libraries VALUES('l','词库','',0,1,1,1)", [])
        .unwrap();
    conn.execute("INSERT INTO library_words VALUES('l','a',1)", [])
        .unwrap();
    conn.execute("DELETE FROM libraries WHERE id='l'", [])
        .unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let failure = transaction(&mut conn, |tx| {
        word(tx, "rolled-back");
        Err::<(), _>(crate::error::AppError::validation("保存失败"))
    })
    .unwrap_err();
    assert_eq!(failure.code, "VALIDATION_FAILED");
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(conn);
    assert!(open_database(&path).is_ok());
    assert!(!dir.path().join("backups").exists());
}

#[test]
fn rejected_files_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    std::fs::write(&path, b"not a sqlite database").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(open_database(&path).unwrap_err().code, "DB_CORRUPT");
    assert_eq!(before, std::fs::read(&path).unwrap());
    std::fs::remove_file(&path).unwrap();
    for statement in [
        "PRAGMA user_version=99",
        "UPDATE schema_migrations SET checksum='modified' WHERE version=1",
        "DELETE FROM schema_migrations WHERE version=1",
        "PRAGMA application_id=0",
    ] {
        let conn = open_database(&path).unwrap();
        conn.execute_batch(statement).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        assert!(open_database(&path).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        std::fs::remove_file(&path).unwrap();
    }
}

#[test]
fn queue_revision_failure_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let service = StorageService::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(service.status().unwrap().word_count, Some(0));
    assert_eq!(
        service
            .save_setting("daily_new_limit".into(), "20".into(), None)
            .unwrap(),
        1
    );
    std::thread::scope(|scope| {
        let a =
            scope.spawn(|| service.save_setting("daily_new_limit".into(), "30".into(), Some(1)));
        let b =
            scope.spawn(|| service.save_setting("daily_new_limit".into(), "40".into(), Some(1)));
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
            "REVISION_CONFLICT"
        );
    });
    assert!(service
        .save_setting("unknown".into(), "1".into(), None)
        .is_err());
    assert!(service.status().is_ok());
    drop(service);
    let service = StorageService::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        service
            .save_setting("daily_new_limit".into(), "10".into(), Some(2))
            .unwrap(),
        3
    );
}

#[test]
fn blocked_startup_status_preserves_higher_version() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("data")).unwrap();
    let path = dir.path().join("data/main.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA user_version=99").unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    let state = StorageState::initialize(dir.path().to_path_buf());
    let status = state.status();
    assert_eq!(status.state, "blocked");
    assert_eq!(status.error.unwrap().code, "SCHEMA_TOO_NEW");
    assert_eq!(status.word_count, None);
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn ready_startup_has_consistent_status() {
    let dir = tempfile::tempdir().unwrap();
    let state = StorageState::initialize(dir.path().to_path_buf());
    let status = state.status();
    assert_eq!(status.state, "ready");
    assert_eq!(status.schema_version, Some(MIGRATIONS.len()));
    assert_eq!(status.library_count, Some(0));
    assert_eq!(status.word_count, Some(0));
    assert!(status.error.is_none());
    assert_eq!(state.status().state, "ready");
}

#[test]
fn database_uses_data_directory_without_version_subdirectory() {
    let dir = tempfile::tempdir().unwrap();
    let state = StorageState::initialize(dir.path().to_path_buf());
    assert_eq!(state.status().state, "ready");
    assert!(dir.path().join("data/main.sqlite3").exists());
    assert!(!dir.path().join("v3").exists());
}
