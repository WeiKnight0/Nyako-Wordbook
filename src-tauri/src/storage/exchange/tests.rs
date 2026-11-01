use super::import::{commit, prepare, CommitOptions};
use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn scheduled_restore_switches_only_after_connections_close() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = file(root.path());
    let preview = preview_file(root.path(), &conn, &source);
    commit(
        &mut conn,
        root.path(),
        &preview.task_id,
        options(),
        &super::jobs::Control::default(),
    )
    .unwrap();
    let backup = root.path().join("original.nyakobak");
    super::backup::create(&conn, root.path(), &backup).unwrap();
    conn.execute("UPDATE words SET kana='変更',revision=revision+1", [])
        .unwrap();
    let task = super::format::id();
    super::backup::validate(
        &backup,
        &root.path().join("staging").join(&task).join("restore"),
    )
    .unwrap();
    super::restore::schedule(&conn, root.path(), &task).unwrap();
    assert_eq!(
        conn.query_row("SELECT kana FROM words", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "変更"
    );
    drop(conn);
    super::restore::recover(root.path()).unwrap();
    let conn = Connection::open(root.path().join("data/main.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("SELECT kana FROM words", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ねこ"
    );
    assert!(!root.path().join("restore-journal.json").exists());
}

#[test]
fn backup_snapshot_and_json_export_round_trip_real_files() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = file(root.path());
    let preview = preview_file(root.path(), &conn, &source);
    commit(
        &mut conn,
        root.path(),
        &preview.task_id,
        options(),
        &super::jobs::Control::default(),
    )
    .unwrap();
    conn.execute("UPDATE cards SET suspended=1,due_at=5678", [])
        .unwrap();
    let backup = root.path().join("complete.nyakobak");
    super::backup::create(&conn, root.path(), &backup).unwrap();
    conn.execute("UPDATE words SET kana='変更'", []).unwrap();
    let extracted = root.path().join("extracted");
    super::backup::validate(&backup, &extracted).unwrap();
    let restored = Connection::open(extracted.join("data/main.sqlite3")).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT kana FROM words", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ねこ"
    );
    assert_eq!(
        restored
            .query_row("SELECT suspended,due_at FROM cards", [], |r| Ok((
                r.get::<_, bool>(0)?,
                r.get::<_, i64>(1)?
            )))
            .unwrap(),
        (true, 5678)
    );
    let output = root.path().join("export.json");
    super::export::export(
        &mut conn,
        super::export::ExportOptions {
            path: output.clone(),
            format: "json".into(),
            library_id: options().library_id,
            selected: vec![],
            csv_loss_confirmed: false,
            csv_original: false,
            expected: None,
        },
    )
    .unwrap();
    let bytes = std::fs::read(output).unwrap();
    let document: super::format::Document = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document.words[0].examples[0].sense_index, Some(0));
    assert_eq!(document.words[0].tags, vec!["动物"]);
    assert!(!String::from_utf8(bytes).unwrap().contains("suspended"));
    assert!(document.exported_at.ends_with('Z'));
    let csv = root.path().join("export.csv");
    super::export::export(
        &mut conn,
        super::export::ExportOptions {
            path: csv.clone(),
            format: "csv".into(),
            library_id: options().library_id,
            selected: vec![],
            csv_loss_confirmed: true,
            csv_original: false,
            expected: None,
        },
    )
    .unwrap();
    assert!(std::fs::read(&csv).unwrap().starts_with(b"\xef\xbb\xbf"));
    let imported = preview_file(root.path(), &conn, &csv);
    assert_eq!((imported.valid, imported.errors), (1, 0));
}

fn options() -> CommitOptions {
    CommitOptions {
        library_id: Some("00000000-0000-0000-0000-000000000001".into()),
        new_library: None,
        valid_only: false,
        choices: vec![],
    }
}

fn database(root: &Path) -> Connection {
    std::fs::create_dir_all(root.join("data")).unwrap();
    let conn = crate::storage::migrations::open_database(
        &root.join("data/main.sqlite3"),
        &root.join("backups"),
    )
    .unwrap();
    conn.execute("INSERT INTO libraries(id,name,created_at,updated_at) VALUES('00000000-0000-0000-0000-000000000001','测试',1,1)", []).unwrap();
    conn
}

fn file(root: &Path) -> std::path::PathBuf {
    let file = root.join("source.json");
    std::fs::write(&file,serde_json::to_vec(&serde_json::json!({"format":"nyako-wordbook","version":1,"source_namespace":"test-source","exported_at":"2026-11-01T00:00:00Z","words":[{"external_id":"cat","kana":"ねこ","senses":[{"meaning":"猫"}],"examples":[{"jp":"猫だ","sense_index":0}],"tags":["动物"]}]})).unwrap()).unwrap();
    file
}

fn preview_file(root: &Path, conn: &Connection, file: &Path) -> Preview {
    let result = preview(
        root,
        file,
        &ImportOptions {
            source_namespace: "unused".into(),
            csv: CsvOptions::default(),
        },
    )
    .unwrap();
    prepare(conn, root, &result.task_id).unwrap();
    result
}

#[test]
fn repeated_import_preserves_cards_and_child_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = file(root.path());
    let first = preview_file(root.path(), &conn, &source);
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &first.task_id,
            options(),
            &super::jobs::Control::default()
        )
        .unwrap()
        .created,
        1
    );
    conn.execute("UPDATE cards SET suspended=1,due_at=1234", [])
        .unwrap();
    let card: String = conn
        .query_row("SELECT id FROM cards", [], |r| r.get(0))
        .unwrap();
    let sense: String = conn
        .query_row("SELECT id FROM senses", [], |r| r.get(0))
        .unwrap();
    let second = preview_file(root.path(), &conn, &source);
    let rows = super::import::page(&conn, root.path(), &second.task_id, 0, false).unwrap();
    assert_eq!(rows[0].classification, "same");
    assert!(!rows[0].locally_modified);
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &second.task_id,
            options(),
            &super::jobs::Control::default()
        )
        .unwrap()
        .unchanged,
        1
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT id FROM senses", [], |r| r.get::<_, String>(0))
            .unwrap(),
        sense
    );
    assert_eq!(
        conn.query_row("SELECT id,suspended,due_at FROM cards", [], |r| Ok((
            r.get::<_, String>(0)?,
            r.get::<_, bool>(1)?,
            r.get::<_, i64>(2)?
        )))
        .unwrap(),
        (card, true, 1234)
    );
}

#[test]
fn backup_rejects_forged_manifest_and_unsafe_paths() {
    use std::io::{Read, Write};
    let root = tempfile::tempdir().unwrap();
    let conn = database(root.path());
    let original = root.path().join("original.nyakobak");
    super::backup::create(&conn, root.path(), &original).unwrap();
    for (case, path) in [
        ("statistics", "data/main.sqlite3"),
        ("traversal", "media/../escape"),
    ] {
        let forged = root.path().join(format!("{case}.nyakobak"));
        let mut input = zip::ZipArchive::new(std::fs::File::open(&original).unwrap()).unwrap();
        let mut output = zip::ZipWriter::new(std::fs::File::create(&forged).unwrap());
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "manifest.json" {
                let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                manifest["words"] = 99.into();
                manifest["files"][0]["path"] = path.into();
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            output
                .start_file(
                    if name == "data/main.sqlite3" {
                        path
                    } else {
                        &name
                    },
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            output.write_all(&bytes).unwrap();
        }
        output.finish().unwrap();
        assert_eq!(
            super::backup::validate(&forged, &root.path().join(case))
                .unwrap_err()
                .code,
            "INVALID_BACKUP"
        );
    }
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn partial_update_keeps_child_ids_and_bom_duplicates_are_all_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = file(root.path());
    let first = preview_file(root.path(), &conn, &source);
    commit(
        &mut conn,
        root.path(),
        &first.task_id,
        options(),
        &jobs::Control::default(),
    )
    .unwrap();
    let word: String = conn
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    let before = crate::storage::words::get(&conn, &word).unwrap().content;
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&source).unwrap()).unwrap();
    document["words"][0]["note"] = "只更新备注".into();
    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend(serde_json::to_vec(&document).unwrap());
    std::fs::write(&source, bytes).unwrap();
    let next = preview_file(root.path(), &conn, &source);
    let mut selected = options();
    selected.choices.push(super::import::Choice {
        line: 1,
        action: "reuse".into(),
        target: Some(word.clone()),
        strategy: "replace".into(),
        fields: ["note".into()].into_iter().collect(),
        clear: Default::default(),
    });
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &next.task_id,
            selected,
            &jobs::Control::default()
        )
        .unwrap()
        .updated,
        1
    );
    let after = crate::storage::words::get(&conn, &word).unwrap().content;
    assert_eq!(before.senses, after.senses);
    assert_eq!(before.examples, after.examples);
    assert_eq!(after.note, "只更新备注");
    let duplicate = document["words"][0].clone();
    document["words"].as_array_mut().unwrap().push(duplicate);
    std::fs::write(&source, serde_json::to_vec(&document).unwrap()).unwrap();
    let duplicate = preview_file(root.path(), &conn, &source);
    assert_eq!((duplicate.valid, duplicate.errors), (0, 2));
}

#[test]
fn unfinished_preview_reopens_and_explicit_resume_rechecks_source() {
    let root = tempfile::tempdir().unwrap();
    let conn = database(root.path());
    let source = file(root.path());
    let task = preview_file(root.path(), &conn, &source);
    drop(conn);
    let conn = Connection::open(root.path().join("data/main.sqlite3")).unwrap();
    let pending = super::import::pending(&conn, root.path()).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].task_id, task.task_id);
    assert_eq!(
        super::import::resume(&conn, root.path(), &task.task_id)
            .unwrap()
            .valid,
        1
    );
    std::fs::write(source, "changed").unwrap();
    assert_eq!(
        super::import::resume(&conn, root.path(), &task.task_id)
            .unwrap_err()
            .code,
        "IMPORT_SOURCE_CHANGED"
    );
}

#[test]
fn stale_preview_and_cancel_leave_no_business_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = file(root.path());
    let first = preview_file(root.path(), &conn, &source);
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &first.task_id,
            options(),
            &super::jobs::Control {
                cancel: AtomicBool::new(true),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "TASK_CANCELLED"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    conn.execute("UPDATE libraries SET revision=revision+1", [])
        .unwrap();
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &first.task_id,
            options(),
            &super::jobs::Control::default()
        )
        .unwrap_err()
        .code,
        "IMPORT_TARGET_CHANGED"
    );
    let second = preview_file(root.path(), &conn, &source);
    std::fs::write(&source, "{}").unwrap();
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &second.task_id,
            options(),
            &super::jobs::Control::default()
        )
        .unwrap_err()
        .code,
        "IMPORT_SOURCE_CHANGED"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM import_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
