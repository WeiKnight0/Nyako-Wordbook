use super::import::{commit, prepare, CommitOptions};
use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn import_change_marker_tracks_local_external_and_reopened_connections() {
    let root = tempfile::tempdir().unwrap();
    let conn = database(root.path());
    let initial = import::snapshot(&conn).unwrap();
    assert_eq!(initial, import::snapshot(&conn).unwrap());
    conn.execute("UPDATE libraries SET description='changed'", [])
        .unwrap();
    let local = import::snapshot(&conn).unwrap();
    assert_ne!(initial, local);
    let path = conn.path().unwrap().to_owned();
    let external = Connection::open(&path).unwrap();
    external
        .execute("UPDATE libraries SET description='external'", [])
        .unwrap();
    assert_ne!(local, import::snapshot(&conn).unwrap());
    let before_reopen = import::snapshot(&conn).unwrap();
    drop(external);
    drop(conn);
    let reopened = Connection::open(path).unwrap();
    assert_ne!(before_reopen, import::snapshot(&reopened).unwrap());
}

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
        Some(&task.source_namespace),
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
fn excel_template_uses_each_user_supplied_namespace() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let source = root.path().join("template.xlsx");
    super::excel_template::save(&source, None, &jobs::Control::default()).unwrap();
    for (namespace, expected_matched) in [
        ("user-library-a", false),
        ("user-library-b", false),
        ("user-library-a", true),
    ] {
        let task = preview(
            root.path(),
            &source,
            &ImportOptions {
                source_namespace: namespace.into(),
                excel: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(task.source_namespace, namespace);
        prepare(&conn, root.path(), &task.task_id).unwrap();
        let rows = super::import::page(&conn, root.path(), &task.task_id, 0, false).unwrap();
        assert_eq!(rows[0].matched.is_some(), expected_matched);
        if expected_matched {
            continue;
        }
        let mut target = options();
        target.library_id = None;
        target.new_library = Some(namespace.into());
        target.choices.push(super::import::Choice {
            line: 2,
            action: "new".into(),
            target: None,
            strategy: "keep".into(),
            fields: Default::default(),
            clear: Default::default(),
        });
        assert_eq!(
            commit(
                &mut conn,
                root.path(),
                &task.task_id,
                target,
                &jobs::Control::default()
            )
            .unwrap()
            .created,
            1
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT count(DISTINCT word_id) FROM source_links WHERE external_id='sample-cat-001'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn json_export_rejects_oversized_local_aggregate_before_publishing() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let document: super::format::Document =
        serde_json::from_slice(&std::fs::read(file(root.path())).unwrap()).unwrap();
    let mut input = document.words[0].input().unwrap();
    input.senses = (0..40)
        .map(|_| crate::storage::models::Sense {
            id: super::format::id(),
            meaning: "释".repeat(10_000),
        })
        .collect();
    input.examples.clear();
    let saved = crate::storage::words::save(&mut conn, input, None).unwrap();
    for existing in [false, true] {
        let output = root.path().join(if existing {
            "existing.json"
        } else {
            "new.json"
        });
        if existing {
            std::fs::write(&output, b"original file").unwrap();
        }
        let error = super::export::export(
            &mut conn,
            super::export::ExportOptions {
                expected: super::output::fingerprint(&output).unwrap(),
                path: output.clone(),
                format: "json".into(),
                library_id: None,
                selected: vec![saved.content.id.clone()],
            },
        )
        .unwrap_err();
        assert!(
            error.message.contains("单个词条超过 1MiB"),
            "{}",
            error.message
        );
        assert!(error.message.contains(&saved.content.id));
        if existing {
            assert_eq!(std::fs::read(&output).unwrap(), b"original file");
        } else {
            assert!(!output.exists());
        }
        assert_eq!(
            conn.query_row("SELECT count(*) FROM source_links", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM settings WHERE key='export_namespace'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(!std::fs::read_dir(root.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nyako-export-")));
    }
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
    assert_eq!(task.source_namespace, "plain");
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
        super::excel::write(&path, Some("test"), &[word], &jobs::Control::default())
            .unwrap_err()
            .message
            .contains("编号行格式冲突")
    );
}

#[test]
fn excel_namespace_metadata_reads_only_escaped_string_constants() {
    let root = tempfile::tempdir().unwrap();
    let source = file(root.path());
    let document: super::format::Document =
        serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    let path = root.path().join("namespace.xlsx");
    let namespace = "来源\"甲\"与中文";
    super::excel::write(
        &path,
        Some(namespace),
        &document.words,
        &jobs::Control::default(),
    )
    .unwrap();
    let (actual, _, _) = super::excel::parse(
        &std::fs::read(&path).unwrap(),
        &Default::default(),
        "ignored",
        &jobs::Control::default(),
    )
    .unwrap();
    assert_eq!(actual, namespace);
    let mut book = rust_xlsxwriter::Workbook::new();
    let sheet = book.add_worksheet();
    for (column, label) in super::excel::LABELS.iter().enumerate() {
        sheet.write_string(0, column as u16, *label).unwrap();
    }
    book.define_name("_nyako_source_namespace", "=1+1").unwrap();
    book.save(&path).unwrap();
    assert!(super::excel::parse(
        &std::fs::read(path).unwrap(),
        &Default::default(),
        "fallback",
        &jobs::Control::default()
    )
    .unwrap_err()
    .message
    .contains("文本常量"));
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
        Some("multi"),
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
        Some("multi"),
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
fn verb_transitivity_survives_excel_and_json_roundtrips() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = database(root.path());
    let mut word = super::format::ExchangeWord {
        external_id: Some("verb-one".into()),
        kanji: "開く".into(),
        kana: "あく".into(),
        parts_of_speech: vec!["godan_verb".into(), "intransitive".into()],
        jlpt: Some("N4".into()),
        pitch_accents: vec![],
        senses: vec![super::format::ExchangeSense {
            meaning: "开".into(),
        }],
        examples: vec![],
        note: String::new(),
    };
    let source = root.path().join("verbs.xlsx");
    super::excel::write(
        &source,
        Some("verbs"),
        &[word.clone()],
        &jobs::Control::default(),
    )
    .unwrap();
    let task = preview_file(root.path(), &conn, &source);
    assert_eq!((task.valid, task.errors), (1, 0));
    commit(
        &mut conn,
        root.path(),
        &task.task_id,
        options(),
        &jobs::Control::default(),
    )
    .unwrap();
    let word_id: String = conn
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        crate::storage::words::get(&conn, &word_id)
            .unwrap()
            .content
            .pos,
        word.parts_of_speech
    );
    let output = root.path().join("verbs.json");
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
    let second = tempfile::tempdir().unwrap();
    let mut restored = database(second.path());
    let task = preview_file(second.path(), &restored, &output);
    commit(
        &mut restored,
        second.path(),
        &task.task_id,
        options(),
        &jobs::Control::default(),
    )
    .unwrap();
    let restored_id: String = restored
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        crate::storage::words::get(&restored, &restored_id)
            .unwrap()
            .content
            .pos,
        word.parts_of_speech
    );
    word.parts_of_speech = vec![
        "godan_verb".into(),
        "intransitive".into(),
        "transitive".into(),
    ];
    assert!(super::excel::write(
        &root.path().join("invalid.xlsx"),
        Some("verbs"),
        &[word],
        &jobs::Control::default()
    )
    .is_err());
}

#[test]
fn unchanged_import_membership_invalidates_old_edit_without_touching_study_data() {
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
    conn.execute("UPDATE words SET updated_at=1", []).unwrap();
    conn.execute("UPDATE cards SET suspended=1,due_at=1234,revision=7", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sessions VALUES('session','{}',1,NULL,'active','UTC',4)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO review_events SELECT 'event',id,'session','rating',3,123,'2026-11-01',1000,'{}','{}','6.6.2','fsrs6-default',NULL,0 FROM cards", []).unwrap();
    fn snapshot(conn: &Connection, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
        let mut statement = conn
            .prepare(&format!("SELECT * FROM {table} ORDER BY id"))
            .unwrap();
        let columns = statement.column_count();
        statement
            .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    let cards = snapshot(&conn, "cards");
    let events = snapshot(&conn, "review_events");
    let word_id: String = conn
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    let before = crate::storage::words::get(&conn, &word_id).unwrap();
    let second = preview_file(root.path(), &conn, &source);
    let mut target = options();
    target.library_id = None;
    target.new_library = Some("新增词库".into());
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &second.task_id,
            target,
            &jobs::Control::default()
        )
        .unwrap()
        .unchanged,
        1
    );
    let after = crate::storage::words::get(&conn, &word_id).unwrap();
    assert_eq!(after.revision, before.revision + 1);
    assert!(after.updated_at > before.updated_at);
    assert_eq!(after.content.library_ids.len(), 2);
    assert_eq!(
        crate::storage::words::save(&mut conn, before.content, Some(before.revision))
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(
        crate::storage::words::get(&conn, &word_id).unwrap().content,
        after.content
    );
    let repeated = preview_file(root.path(), &conn, &source);
    let mut target = options();
    target.library_id = after
        .content
        .library_ids
        .iter()
        .find(|id| Some(*id) != options().library_id.as_ref())
        .cloned();
    assert_eq!(
        commit(
            &mut conn,
            root.path(),
            &repeated.task_id,
            target,
            &jobs::Control::default()
        )
        .unwrap()
        .unchanged,
        1
    );
    let repeated = crate::storage::words::get(&conn, &word_id).unwrap();
    assert_eq!(repeated.revision, after.revision);
    assert_eq!(repeated.updated_at, after.updated_at);
    assert_eq!(cards, snapshot(&conn, "cards"));
    assert_eq!(events, snapshot(&conn, "review_events"));
}

#[test]
fn json_roundtrip_preserves_maximum_unicode_notes() {
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
    let word_id: String = conn
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    for character in ["备", "𠀀"] {
        let mut detail = crate::storage::words::get(&conn, &word_id).unwrap();
        detail.content.note = character.repeat(100_000);
        let saved =
            crate::storage::words::save(&mut conn, detail.content, Some(detail.revision)).unwrap();
        let output = root.path().join(format!("note-{}.json", saved.revision));
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
        let second = tempfile::tempdir().unwrap();
        let mut restored = database(second.path());
        let task = preview_file(second.path(), &restored, &output);
        assert_eq!((task.valid, task.errors), (1, 0));
        commit(
            &mut restored,
            second.path(),
            &task.task_id,
            options(),
            &jobs::Control::default(),
        )
        .unwrap();
        assert_eq!(
            restored
                .query_row("SELECT note FROM words", [], |row| row.get::<_, String>(0))
                .unwrap(),
            saved.content.note
        );
        let imported = preview_file(root.path(), &conn, &output);
        assert_eq!(
            commit(
                &mut conn,
                root.path(),
                &imported.task_id,
                options(),
                &jobs::Control::default()
            )
            .unwrap()
            .unchanged,
            1
        );
        assert_eq!(
            crate::storage::words::get(&conn, &word_id)
                .unwrap()
                .revision,
            saved.revision
        );
    }
    let mut word =
        super::import::portable(&crate::storage::words::get(&conn, &word_id).unwrap().content);
    word.note.push('备');
    assert!(word.validate().is_err());
}

#[test]
fn excel_export_retains_namespace_when_spelling_and_reading_change() {
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
    let output = root.path().join("identity.xlsx");
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
    let exported = preview_file(root.path(), &conn, &output);
    let namespace: String = serde_json::from_str(
        &conn
            .query_row(
                "SELECT value_json FROM settings WHERE key='export_namespace'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(exported.source_namespace, namespace);
    let rows = super::import::page(&conn, root.path(), &exported.task_id, 0, false).unwrap();
    let word_id = rows[0].matched.clone().expect("导出身份应直接匹配原词条");
    let second = tempfile::tempdir().unwrap();
    let mut restored = database(second.path());
    let imported = preview_file(second.path(), &restored, &output);
    commit(
        &mut restored,
        second.path(),
        &imported.task_id,
        options(),
        &jobs::Control::default(),
    )
    .unwrap();
    let restored_id: String = restored
        .query_row("SELECT id FROM words", [], |row| row.get(0))
        .unwrap();
    let mut changed =
        serde_json::from_value::<super::format::ExchangeWord>(rows[0].content.clone().unwrap())
            .unwrap();
    changed.kanji = "小猫".into();
    changed.kana = "こねこ".into();
    super::excel::write(
        &output,
        Some(&namespace),
        &[changed],
        &jobs::Control::default(),
    )
    .unwrap();
    for (database, directory, expected_id) in [
        (&mut conn, root.path(), word_id),
        (&mut restored, second.path(), restored_id),
    ] {
        let task = preview_file(directory, database, &output);
        let rows = super::import::page(database, directory, &task.task_id, 0, false).unwrap();
        assert_eq!(rows[0].classification, "update");
        assert_eq!(rows[0].matched.as_ref(), Some(&expected_id));
        let mut target = options();
        target.choices.push(super::import::Choice {
            line: 2,
            action: "reuse".into(),
            target: Some(expected_id.clone()),
            strategy: "replace".into(),
            fields: ["kanji".into(), "kana".into()].into_iter().collect(),
            clear: Default::default(),
        });
        assert_eq!(
            commit(
                database,
                directory,
                &task.task_id,
                target,
                &jobs::Control::default()
            )
            .unwrap()
            .updated,
            1
        );
        assert_eq!(
            database
                .query_row("SELECT count(*) FROM words", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            crate::storage::words::get(database, &expected_id)
                .unwrap()
                .content
                .kana,
            "こねこ"
        );
    }
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
fn import_summary_covers_all_pages_and_preserves_field_presence() {
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
    conn.execute("INSERT INTO source_links SELECT source_namespace,'cat-update',word_id,last_import_hash FROM source_links WHERE external_id='cat'", []).unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&source).unwrap()).unwrap();
    let same = document["words"][0].clone();
    let mut rows = (0..51).map(|index| serde_json::json!({"external_id":format!("new-{index}"),"kanji":format!("新{index}"),"kana":format!("しん{index}"),"senses":[{"meaning":"新词"}]})).collect::<Vec<_>>();
    rows.push(same.clone());
    let mut update = same.clone();
    update["external_id"] = "cat-update".into();
    update["note"] = "已提供的更新".into();
    rows.push(update);
    let mut unlinked = same.clone();
    unlinked["external_id"] = "unlinked-same".into();
    unlinked["note"] = "".into();
    rows.push(unlinked);
    let mut ambiguous = same;
    ambiguous["external_id"] = "unlinked-different".into();
    ambiguous["note"] = "不同的内容".into();
    rows.push(ambiguous);
    rows.push(serde_json::json!({"external_id":"invalid","kanji":"错","kana":"あ","senses":[]}));
    document["words"] = rows.into();
    std::fs::write(&source, serde_json::to_vec(&document).unwrap()).unwrap();
    let task = preview_file(root.path(), &conn, &source);
    let control = jobs::Control::default();
    let summary = super::execute(
        &mut conn,
        root.path(),
        ExchangeRequest::ImportSummary {
            task_id: task.task_id.clone(),
        },
        &control,
    )
    .unwrap();
    assert_eq!(
        summary,
        serde_json::json!({"new":51,"same":2,"update":1,"ambiguous":1,"error":1,"unresolved_lines":[54,55],"source_namespace":"test-source"})
    );
    assert_eq!(
        control.processed.load(std::sync::atomic::Ordering::Acquire),
        56
    );
    let page = super::import::page(&conn, root.path(), &task.task_id, 50, false).unwrap();
    assert!(!page[1].present.contains("note"));
    assert!(page[3].present.contains("note"));
    assert!(page[5].present.is_empty());
    assert_eq!(
        super::import::summary(
            &conn,
            root.path(),
            &task.task_id,
            &jobs::Control {
                cancel: AtomicBool::new(true),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "TASK_CANCELLED"
    );
    conn.execute("UPDATE words SET revision=revision+1", [])
        .unwrap();
    assert_eq!(
        super::import::summary(&conn, root.path(), &task.task_id, &jobs::Control::default())
            .unwrap_err()
            .code,
        "IMPORT_TARGET_CHANGED"
    );
}

#[test]
fn unfinished_preview_reopens_and_explicit_resume_rechecks_source() {
    let root = tempfile::tempdir().unwrap();
    let conn = database(root.path());
    let source = file(root.path());
    let task = preview_file(root.path(), &conn, &source);
    assert_eq!(task.path, source);
    super::import::stage(root.path(), &task.task_id)
        .unwrap()
        .execute(
            "UPDATE metadata SET value=json_remove(value,'$.path') WHERE key='preview'",
            [],
        )
        .unwrap();
    drop(conn);
    let conn = Connection::open(root.path().join("data/main.sqlite3")).unwrap();
    let pending = super::import::pending(&conn, root.path()).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].task_id, task.task_id);
    assert_eq!(pending[0].path, source);
    let resumed = super::import::resume(&conn, root.path(), &task.task_id).unwrap();
    assert_eq!(resumed.valid, 1);
    assert_eq!(resumed.path, source);
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
