use super::import::{commit, prepare, CommitOptions};
use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn excel_single_sheet_roundtrip_and_reimport_preserve_cards() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = root.path().join("source.xlsx");
    super::excel_template::save(&source, None, &jobs::Control::default()).unwrap();
    let task = preview_file(root.path(), &conn, &source);
    commit(
        &mut conn,
        root.path(),
        &task.task_id,
        options(),
        &jobs::Control::default(),
    )
    .unwrap();
    let output = root.path().join("words.xlsx");
    super::export::export(
        &mut conn,
        super::export::ExportOptions {
            path: output.clone(),
            format: "xlsx".into(),
            library_id: options().library_id,
            selected: vec![],
            expected: None,
        },
    )
    .unwrap();
    let archive = std::fs::read(&output).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
    let mut styles = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("xl/styles.xml").unwrap(), &mut styles).unwrap();
    assert!(styles.contains("Microsoft YaHei"));
    assert!(styles.contains("Yu Gothic"));
    assert_eq!(
        zip.file_names()
            .filter(|name| {
                name.as_ref().is_ok_and(|name| {
                    name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml")
                })
            })
            .count(),
        1
    );
    let task = preview_file(root.path(), &conn, &output);
    let rows = super::import::page(&conn, root.path(), &task.task_id, 0, false).unwrap();
    assert_eq!(rows[0].classification, "same");
    let card: String = conn
        .query_row("SELECT id FROM cards", [], |row| row.get(0))
        .unwrap();
    let word_id: String = conn
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    let mut selected = options();
    selected.choices.push(super::import::Choice {
        line: 2,
        action: "reuse".into(),
        target: Some(word_id),
        strategy: "keep".into(),
        fields: Default::default(),
        clear: Default::default(),
    });
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &task.task_id,
            selected,
            &jobs::Control::default()
        )
        .unwrap()
        .unchanged,
        1
    );
    assert_eq!(
        conn.query_row("SELECT id FROM cards", [], |row| row.get::<_, String>(0))
            .unwrap(),
        card
    );
    let empty = tempfile::tempdir().unwrap();
    let mut second = database(empty.path());
    let task = preview_file(empty.path(), &second, &output);
    assert_eq!(
        commit(
            &mut second,
            empty.path(),
            &task.task_id,
            options(),
            &jobs::Control::default()
        )
        .unwrap()
        .created,
        1
    );
    let content = super::import::page(&second, empty.path(), &task.task_id, 0, false).unwrap();
    assert_eq!(
        content[0].content.as_ref().unwrap()["examples"][0]["sense_index"],
        0
    );
    let word_id: String = second
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    let mut changed = super::import::portable(
        &crate::storage::words::get(&second, &word_id)
            .unwrap()
            .content,
    );
    changed.external_id = Some(
        content[0].content.as_ref().unwrap()["external_id"]
            .as_str()
            .unwrap()
            .into(),
    );
    changed.senses[0].meaning = "猫科动物".into();
    changed.note = "Excel 修改后的备注".into();
    super::excel::write(
        &output,
        &task.source_namespace,
        &[changed],
        &jobs::Control::default(),
    )
    .unwrap();
    let updated = preview_file(empty.path(), &second, &output);
    let mut selected = options();
    selected.choices.push(super::import::Choice {
        line: 2,
        action: "reuse".into(),
        target: Some(word_id.clone()),
        strategy: "replace".into(),
        fields: ["senses".into(), "examples".into(), "note".into()]
            .into_iter()
            .collect(),
        clear: Default::default(),
    });
    assert_eq!(
        commit(
            &mut second,
            empty.path(),
            &updated.task_id,
            selected,
            &jobs::Control::default()
        )
        .unwrap()
        .updated,
        1
    );
    let detail = crate::storage::words::get(&second, &word_id).unwrap();
    assert_eq!(detail.content.senses.len(), 1);
    assert_eq!(detail.content.senses[0].meaning, "猫科动物");
    assert_eq!(
        detail.content.examples[0].sense_id.as_ref(),
        Some(&detail.content.senses[0].id)
    );
    assert_eq!(detail.content.note, "Excel 修改后的备注");
}

#[test]
fn excel_plain_chinese_sheet_maps_and_rejects_formulas() {
    let root = tempfile::tempdir().unwrap();
    let conn = database(root.path());
    let path = root.path().join("plain.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let sheet = book.add_worksheet().set_name("单词表").unwrap();
    for (column, label) in super::excel::LABELS.iter().enumerate() {
        sheet.write_string(0, column as u16, *label).unwrap();
    }
    sheet.write_string(1, 1, "犬").unwrap();
    sheet.write_string(1, 2, "いぬ").unwrap();
    sheet.write_string(1, 3, "狗").unwrap();
    sheet.write_number(1, 5, 1.0).unwrap();
    book.save(&path).unwrap();
    let options = ImportOptions {
        source_namespace: "plain".into(),
        excel: super::excel::ExcelOptions {
            sheet: Some("单词表".into()),
            headers: true,
            ..Default::default()
        },
    };
    let task = preview(root.path(), &path, &options).unwrap();
    assert_eq!((task.valid, task.errors), (1, 0));
    let rows = super::import::stage(root.path(), &task.task_id).unwrap();
    let word: String = rows
        .query_row("SELECT content FROM rows", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&word).unwrap()["pitch_accents"][0]["downstep"],
        1
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    book.worksheet_from_name("单词表")
        .unwrap()
        .write_formula(1, 5, "=1+1")
        .unwrap();
    book.save(&path).unwrap();
    assert!(preview(root.path(), &path, &options)
        .unwrap_err()
        .message
        .contains("公式"));
}

#[test]
fn excel_rejects_missing_required_values_and_invalid_pitch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("invalid.xlsx");
    for (column, value, message) in [
        (1, "", "写法、假名、释义"),
        (2, "", "写法、假名、释义"),
        (3, "", "写法、假名、释义"),
        (5, "abc", "音调必须为斜杠分隔的非负整数"),
        (5, "1/", "音调必须为斜杠分隔的非负整数"),
        (5, "3", "音调"),
        (3, "1. 狗\n3. 犬", "释义编号"),
        (7, "1. 犬です\n3. 犬がいます", "日文例句编号"),
        (8, "1. 狗。\n3. 犬。", "例句中文翻译编号"),
    ] {
        let mut book = rust_xlsxwriter::Workbook::new();
        let sheet = book.add_worksheet();
        for (index, label) in super::excel::LABELS.iter().enumerate() {
            sheet.write_string(0, index as u16, *label).unwrap();
        }
        for (index, content) in [
            (1, "犬"),
            (2, "いぬ"),
            (3, "狗"),
            (5, "1"),
            (
                7,
                if column == 8 {
                    "1. 犬です\n2. 犬がいます"
                } else {
                    "犬です"
                },
            ),
            (8, ""),
        ] {
            sheet
                .write_string(1, index, if index == column { value } else { content })
                .unwrap();
        }
        book.save(&path).unwrap();
        let task = preview(
            root.path(),
            &path,
            &ImportOptions {
                source_namespace: "test-excel".into(),
                excel: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(
            (task.valid, task.errors),
            (0, 1),
            "column={column} value={value:?}"
        );
        let rows = super::import::stage(root.path(), &task.task_id).unwrap();
        let error: String = rows
            .query_row("SELECT error FROM rows", [], |row| row.get(0))
            .unwrap();
        assert!(error.contains(message), "{error}");
    }
    let mut book = rust_xlsxwriter::Workbook::new();
    book.add_worksheet();
    book.add_worksheet();
    book.save(&path).unwrap();
    assert!(preview(
        root.path(),
        &path,
        &ImportOptions {
            source_namespace: "test-excel".into(),
            excel: Default::default(),
        }
    )
    .unwrap_err()
    .message
    .contains("一个工作表"));
}

#[test]
fn excel_rejects_numbered_meaning_conflict() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("complex.xlsx");
    let word = super::format::ExchangeWord {
        external_id: Some("id".into()),
        kanji: "猫".into(),
        kana: "ねこ".into(),
        senses: vec![super::format::ExchangeSense {
            meaning: "1. 与编号冲突".into(),
        }],
        parts_of_speech: vec![],
        pitch_accents: vec![],
        jlpt: None,
        examples: vec![],
        note: String::new(),
    };
    assert!(
        super::excel::write(&path, "test", &[word], &jobs::Control::default())
            .unwrap_err()
            .message
            .contains("编号行格式冲突")
    );
}

#[test]
fn excel_multiple_senses_pitches_and_examples_roundtrip() {
    use super::format::{ExchangeExample, ExchangePitch, ExchangeSense, ExchangeWord};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("multiple.xlsx");
    let word = ExchangeWord {
        external_id: Some("multi".into()),
        kanji: "犬".into(),
        kana: "いぬ".into(),
        senses: vec![
            ExchangeSense {
                meaning: "狗".into(),
            },
            ExchangeSense {
                meaning: "犬类".into(),
            },
        ],
        parts_of_speech: vec!["noun".into()],
        pitch_accents: vec![ExchangePitch { downstep: 0 }, ExchangePitch { downstep: 1 }],
        jlpt: None,
        examples: vec![
            ExchangeExample {
                jp: "犬がいる。".into(),
                cn: Some("有狗。".into()),
                sense_index: Some(0),
            },
            ExchangeExample {
                jp: "犬を見た。".into(),
                cn: None,
                sense_index: Some(1),
            },
            ExchangeExample {
                jp: "犬が走る。".into(),
                cn: Some("狗在跑。".into()),
                sense_index: None,
            },
        ],
        note: "备注".into(),
    };
    super::excel::write(
        &path,
        "multi",
        std::slice::from_ref(&word),
        &jobs::Control::default(),
    )
    .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let inspection =
        super::excel::inspect(&bytes, &Default::default(), &jobs::Control::default()).unwrap();
    assert_eq!(inspection["samples"][0][3], "1. 狗\n2. 犬类");
    assert_eq!(inspection["samples"][0][5], "0/1");
    assert_eq!(
        inspection["samples"][0][7],
        "1. 犬がいる。\n2. [义项2] 犬を見た。\n3. [无关联] 犬が走る。"
    );
    assert_eq!(inspection["samples"][0][8], "1. 有狗。\n2.\n3. 狗在跑。");
    let (_, parsed, _) = super::excel::parse(
        &bytes,
        &Default::default(),
        "multi",
        &jobs::Control::default(),
    )
    .unwrap();
    assert_eq!(parsed[0].1.as_ref().unwrap(), &word);
    let mut untranslated = word.clone();
    untranslated
        .examples
        .iter_mut()
        .for_each(|example| example.cn = None);
    let blank_path = root.path().join("untranslated.xlsx");
    super::excel::write(
        &blank_path,
        "multi",
        &[untranslated.clone()],
        &jobs::Control::default(),
    )
    .unwrap();
    let (_, blank, _) = super::excel::parse(
        &std::fs::read(&blank_path).unwrap(),
        &Default::default(),
        "multi",
        &jobs::Control::default(),
    )
    .unwrap();
    assert_eq!(blank[0].1.as_ref().unwrap(), &untranslated);
    let mut conn = database(root.path());
    let task = preview_file(root.path(), &conn, &path);
    assert_eq!((task.valid, task.errors), (1, 0));
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &task.task_id,
            options(),
            &jobs::Control::default()
        )
        .unwrap()
        .created,
        1
    );
    let id: String = conn
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    let content = super::import::portable(&crate::storage::words::get(&conn, &id).unwrap().content);
    assert_eq!(content.senses, word.senses);
    let mut actual_pitches = content
        .pitch_accents
        .iter()
        .map(|pitch| pitch.downstep)
        .collect::<Vec<_>>();
    actual_pitches.sort_unstable();
    assert_eq!(actual_pitches, vec![0, 1]);
    assert_eq!(content.examples, word.examples);
}

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
            expected: None,
        },
    )
    .unwrap();
    let bytes = std::fs::read(output).unwrap();
    let document: super::format::Document = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document.words[0].examples[0].sense_index, Some(0));
    assert_eq!(document.version, 2);
    assert!(!String::from_utf8(bytes).unwrap().contains("suspended"));
    assert!(document.exported_at.ends_with('Z'));
    let imported = preview_file(root.path(), &conn, &root.path().join("export.json"));
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
    std::fs::write(&file,serde_json::to_vec(&serde_json::json!({"format":"nyako-wordbook","version":2,"source_namespace":"test-source","exported_at":"2026-11-01T00:00:00Z","words":[{"external_id":"cat","kanji":"猫","kana":"ねこ","senses":[{"meaning":"猫"}],"examples":[{"jp":"猫だ","sense_index":0}]}]})).unwrap()).unwrap();
    file
}

fn preview_file(root: &Path, conn: &Connection, file: &Path) -> Preview {
    let result = preview(
        root,
        file,
        &ImportOptions {
            source_namespace: "unused".into(),
            excel: Default::default(),
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
