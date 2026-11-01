use super::*;
use migrations::{open_database, open_with_scripts, MIGRATIONS};

fn word(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO words(id,kana,created_at,updated_at) VALUES(?1,'はたらく',1,1)",
        [id],
    )
    .unwrap();
    conn.execute("INSERT INTO senses VALUES(?1,?1,0,'工作')", [id])
        .unwrap();
}

#[test]
fn schema_constraints_and_aggregate_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    let mut conn = open_database(&path, &dir.path().join("backups")).unwrap();
    word(&conn, "a");
    word(&conn, "b");
    assert!(conn
        .execute("INSERT INTO examples VALUES('x','a','b',0,'働く',NULL)", [])
        .is_err());
    assert!(conn
        .execute("INSERT INTO pitch_accents VALUES('p','a',2,3,NULL)", [])
        .is_err());
    assert!(conn
        .execute("INSERT INTO word_tags VALUES('a','missing')", [])
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
        tx.execute(
            "INSERT INTO words(id,kana,created_at,updated_at) VALUES('orphan','ねこ',1,1)",
            [],
        )?;
        Ok(())
    })
    .unwrap_err();
    assert_eq!(failure.code, "VALIDATION_FAILED");
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(conn);
    assert!(open_database(&path, &dir.path().join("backups")).is_ok());
    assert!(!dir.path().join("backups").exists());
}

#[test]
fn upgrade_snapshot_includes_wal_and_failed_upgrade_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    let backups = dir.path().join("backups");
    let old = open_with_scripts(&path, &backups, &MIGRATIONS[..1]).unwrap();
    word(&old, "wal-word");
    let conn = open_database(&path, &backups).unwrap();
    let backup = std::fs::read_dir(&backups)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let snapshot = Connection::open(backup).unwrap();
    assert_eq!(
        snapshot
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(snapshot);
    drop(conn);
    drop(old);
    let scripts = [
        MIGRATIONS[0],
        MIGRATIONS[1],
        "CREATE TABLE must_rollback(id); INVALID SQL;",
    ];
    assert_eq!(
        open_with_scripts(&path, &backups, &scripts)
            .unwrap_err()
            .code,
        "MIGRATION_FAILED"
    );
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='must_rollback'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn backup_failure_prevents_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    drop(open_with_scripts(&path, &dir.path().join("backups"), &MIGRATIONS[..1]).unwrap());
    let bad = dir.path().join("not-a-directory");
    std::fs::write(&bad, b"occupied").unwrap();
    assert!(open_database(&path, &bad).is_err());
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn rejected_files_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    std::fs::write(&path, b"not a sqlite database").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        open_database(&path, &dir.path().join("backups"))
            .unwrap_err()
            .code,
        "DB_CORRUPT"
    );
    assert_eq!(before, std::fs::read(&path).unwrap());
    std::fs::remove_file(&path).unwrap();
    for statement in [
        "PRAGMA user_version=99",
        "UPDATE schema_migrations SET checksum='modified' WHERE version=1",
        "DELETE FROM schema_migrations WHERE version=1",
        "PRAGMA application_id=0",
    ] {
        let conn = open_database(&path, &dir.path().join("backups")).unwrap();
        conn.execute_batch(statement).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        assert!(open_database(&path, &dir.path().join("backups")).is_err());
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
    assert_eq!(status.schema_version, Some(2));
    assert_eq!(status.library_count, Some(0));
    assert_eq!(status.word_count, Some(0));
    assert!(status.error.is_none());
    assert_eq!(state.status().state, "ready");
}

#[test]
fn new_database_failed_migration_can_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.sqlite3");
    assert!(open_with_scripts(&path, &dir.path().join("backups"), &["INVALID SQL"]).is_err());
    assert!(!path.exists());
    assert!(open_database(&path, &dir.path().join("backups")).is_ok());
}
