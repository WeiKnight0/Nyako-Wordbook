use super::{libraries, migrations, models::*, query, words};
use rusqlite::Connection;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn content() -> WordInput {
    WordInput {
        id: id(),
        kanji: "働く".into(),
        kana: "はたらく".into(),
        jlpt: Some("N3".into()),
        note: "plain note".into(),
        senses: vec![
            Sense {
                id: id(),
                meaning: "工作；劳动".into(),
            },
            Sense {
                id: id(),
                meaning: "发挥作用".into(),
            },
        ],
        examples: vec![],
        pitches: vec![],
        pos: vec!["godan_verb".into()],
        library_ids: vec![],
    }
}
fn database() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrations::open_database(
        &dir.path().join("main.sqlite3"),
        &dir.path().join("backups"),
    )
    .unwrap();
    query::register(&conn).unwrap();
    (dir, conn)
}
fn filter() -> WordQuery {
    WordQuery {
        text: String::new(),
        library_id: None,
        unclassified: false,
        jlpt: None,
        pos: None,
        state: None,
        due: None,
        sort: "updated_at".into(),
        descending: true,
    }
}
fn library(conn: &mut Connection, name: &str) -> String {
    let value = Library {
        id: id(),
        name: name.into(),
        description: String::new(),
        archived: false,
        revision: 1,
        word_count: 0,
    };
    let key = value.id.clone();
    libraries::save(conn, value, None).unwrap();
    key
}

#[test]
fn editing_reviewed_word_preserves_every_card_and_history_column() {
    let (_dir, mut conn) = database();
    let saved = words::save(&mut conn, content(), None).unwrap();
    conn.execute("UPDATE cards SET state='review',due_at=123456,last_review_at=123,stability=4.5,difficulty=3.2,reps=7,lapses=2,revision=9", []).unwrap();
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
        let count = statement.column_count();
        statement
            .query_map([], |row| (0..count).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    let cards = snapshot(&conn, "cards");
    let events = snapshot(&conn, "review_events");
    let mut changed = saved.content;
    changed.senses[0].meaning = "修改释义".into();
    words::save(&mut conn, changed, Some(saved.revision)).unwrap();
    assert_eq!(cards, snapshot(&conn, "cards"));
    assert_eq!(events, snapshot(&conn, "review_events"));
}

#[test]
fn aggregate_reorders_preserve_ids_examples_and_card() {
    let (_dir, mut conn) = database();
    let mut input = content();
    input.pitches = vec![
        Pitch {
            id: id(),
            downstep: 1,
        },
        Pitch {
            id: id(),
            downstep: 2,
        },
    ];
    input.examples.push(Example {
        id: id(),
        sense_id: Some(input.senses[0].id.clone()),
        jp: "働く".into(),
        cn: None,
    });
    let saved = words::save(&mut conn, input, None).unwrap();
    let card=conn.query_row("SELECT json_object('id',id,'state',state,'due',due_at,'revision',revision,'params',parameter_set_id) FROM cards",[],|r|r.get::<_,String>(0)).unwrap();
    let mut changed = saved.content.clone();
    changed.senses.reverse();
    changed.pitches.reverse();
    let updated = words::save(&mut conn, changed, Some(saved.revision)).unwrap();
    assert_eq!(updated.content.senses[1].id, saved.content.senses[0].id);
    assert_eq!(updated.content.pitches[0].id, saved.content.pitches[1].id);
    let mut changed = updated.content.clone();
    changed
        .senses
        .retain(|s| s.id != saved.content.senses[0].id);
    changed.examples[0].sense_id = None;
    let updated = words::save(&mut conn, changed, Some(updated.revision)).unwrap();
    assert_eq!(updated.content.examples[0].id, saved.content.examples[0].id);
    assert_eq!(updated.content.examples[0].sense_id, None);
    assert_eq!(card,conn.query_row("SELECT json_object('id',id,'state',state,'due',due_at,'revision',revision,'params',parameter_set_id) FROM cards",[],|r|r.get::<_,String>(0)).unwrap());
    assert_eq!(
        words::save(&mut conn, updated.content.clone(), Some(1))
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let mut invalid = updated.content.clone();
    invalid.senses.clear();
    assert!(words::save(&mut conn, invalid, Some(updated.revision)).is_err());
    assert_eq!(
        words::get(&conn, &updated.content.id).unwrap().revision,
        updated.revision
    );
}

#[test]
fn shared_memberships_batch_and_reopen() {
    let (dir, mut conn) = database();
    let first = library(&mut conn, "同名");
    let second = library(&mut conn, "同名");
    let mut input = content();
    input.library_ids = vec![first.clone(), second.clone()];
    let saved = words::save(&mut conn, input, None).unwrap();
    libraries::delete(&mut conn, &first, 1).unwrap();
    let updated = words::get(&conn, &saved.content.id).unwrap();
    assert_eq!(updated.content.library_ids, vec![second]);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM cards", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    let another = words::save(&mut conn, content(), None).unwrap();
    let selected = vec![
        SelectedWord {
            id: updated.content.id.clone(),
            revision: updated.revision,
        },
        SelectedWord {
            id: another.content.id.clone(),
            revision: 999,
        },
    ];
    assert!(words::batch(&mut conn, selected, "delete", None).is_err());
    assert!(words::get(&conn, &updated.content.id).is_ok());
    drop(conn);
    let conn = migrations::open_database(
        &dir.path().join("main.sqlite3"),
        &dir.path().join("backups"),
    )
    .unwrap();
    assert_eq!(
        words::get(&conn, &saved.content.id).unwrap().content.kana,
        "はたらく"
    );
}

#[test]
fn search_filters_boundaries_and_stable_cursors() {
    let (_dir, mut conn) = database();
    for _ in 0..5 {
        words::save(&mut conn, content(), None).unwrap();
    }
    for text in ["ハタラク 工作", "工", "发挥", "plain"] {
        let mut q = filter();
        q.text = text.into();
        assert_eq!(query::list(&conn, q, None, 100).unwrap().total, 5);
    }
    let mut q = filter();
    q.text = "劳动发挥".into();
    assert_eq!(query::list(&conn, q, None, 100).unwrap().total, 0);
    for sort in ["updated_at", "kana", "kanji", "due_at"] {
        for descending in [true, false] {
            let mut q = filter();
            q.sort = sort.into();
            q.descending = descending;
            let mut ids = vec![];
            let mut cursor = None;
            loop {
                let page = query::list(&conn, q.clone(), cursor, 2).unwrap();
                ids.extend(page.words.into_iter().map(|w| w.id));
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), 5);
        }
    }
    let page = query::list(&conn, filter(), None, 2).unwrap();
    let mut q = filter();
    q.jlpt = Some("N5".into());
    assert!(query::list(&conn, q, page.next_cursor, 2).is_err());
    let mut q = filter();
    q.due = Some(true);
    assert_eq!(query::list(&conn, q, None, 100).unwrap().total, 0);
}

#[test]
fn broad_search_fallback_preserves_results_and_cursors() {
    let (_dir, conn) = database();
    conn.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<10001)
        INSERT INTO words(id,kanji,kana,search_text,created_at,updated_at)
        SELECT printf('00000000-0000-4000-8000-%012x',i),'働く','はたらく','働く はたらく',1,i FROM n;
        INSERT INTO senses(id,word_id,position,meaning) SELECT id,id,0,'工作' FROM words;",
    )
    .unwrap();
    let mut q = filter();
    q.text = "ハタラク".into();
    let first = query::list(&conn, q.clone(), None, 100).unwrap();
    assert_eq!(first.total, 10001);
    assert_eq!(first.words.len(), 100);
    let next = query::list(&conn, q, first.next_cursor, 100).unwrap();
    assert_eq!(next.total, 10001);
    assert!(first
        .words
        .iter()
        .all(|word| next.words.iter().all(|other| word.id != other.id)));
}

#[test]
fn search_index_refresh_and_delete_impact() {
    let (_dir, mut conn) = database();
    let saved = words::save(&mut conn, content(), None).unwrap();
    let mut changed = saved.content;
    changed.note = "unique replacement".into();
    let saved = words::save(&mut conn, changed, Some(saved.revision)).unwrap();
    let mut q = filter();
    q.text = "replacement".into();
    assert_eq!(query::list(&conn, q.clone(), None, 100).unwrap().total, 1);
    let selected = vec![SelectedWord {
        id: saved.content.id.clone(),
        revision: saved.revision,
    }];
    assert_eq!(words::impact(&conn, &selected).unwrap().cards, 1);
    words::batch(&mut conn, selected, "delete", None).unwrap();
    assert_eq!(query::list(&conn, q, None, 100).unwrap().total, 0);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM cards", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn foreign_child_ids_and_partial_updates_are_rejected() {
    let (_dir, mut conn) = database();
    let first = words::save(&mut conn, content(), None).unwrap();
    let mut second = content();
    second.senses[0].id = first.content.senses[0].id.clone();
    assert!(words::save(&mut conn, second, None).is_err());
    assert_eq!(
        conn.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM cards", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
