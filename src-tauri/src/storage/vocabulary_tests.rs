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
        meaning: "工作；劳动
发挥作用"
            .into(),
        examples_jp: "毎日働く。

生活する。"
            .into(),
        examples_cn: "每天工作。".into(),
        pitch: "0 / 2；平板型".into(),
        pos: vec!["godan_verb".into()],
        library_ids: vec![],
    }
}
#[test]
fn verb_transitivity_is_optional_exclusive_and_requires_verb_form() {
    let (_dir, mut conn) = database();
    let mut input = content();
    input.pos.push("intransitive".into());
    let saved = words::save(&mut conn, input.clone(), None).unwrap();
    assert_eq!(saved.content.pos, vec!["godan_verb", "intransitive"]);
    input.pos.push("transitive".into());
    assert!(super::validation::word(input.clone()).is_err());
    input.pos = vec!["transitive".into()];
    assert!(super::validation::word(input).is_err());
    assert_eq!(
        words::get(&conn, &saved.content.id).unwrap().content.pos,
        saved.content.pos
    );
}

fn database() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrations::open_database(&dir.path().join("main.sqlite3")).unwrap();
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
    changed.meaning = "修改释义".into();
    words::save(&mut conn, changed, Some(saved.revision)).unwrap();
    assert_eq!(cards, snapshot(&conn, "cards"));
    assert_eq!(events, snapshot(&conn, "review_events"));
}

#[test]
fn text_roundtrip_limits_and_revision_conflicts() {
    let (_dir, mut conn) = database();
    let input = content();
    let saved = words::save(&mut conn, input.clone(), None).unwrap();
    assert_eq!(saved.content, input);
    let mut changed = input;
    changed.meaning = "1. 工作

随意编号，不配对例句"
        .into();
    let updated = words::save(&mut conn, changed.clone(), Some(1)).unwrap();
    assert_eq!(updated.content, changed);
    assert!(words::save(&mut conn, changed.clone(), Some(1)).is_err());
    changed.meaning.clear();
    assert!(words::save(&mut conn, changed, Some(updated.revision)).is_err());
    assert_eq!(
        words::get(&conn, &updated.content.id).unwrap().content,
        updated.content
    );
    for field in ["meaning", "examples_jp", "examples_cn", "note"] {
        let mut json = serde_json::to_value(content()).unwrap();
        json[field] = "文".repeat(100_000).into();
        let value: WordInput = serde_json::from_value(json.clone()).unwrap();
        super::validation::word(value).unwrap();
        json[field] = "文".repeat(100_001).into();
        assert!(super::validation::word(serde_json::from_value(json).unwrap()).is_err());
    }
    let mut irregular = content();
    irregular.pos = vec!["irregular_verb".into(), "transitive".into()];
    words::save(&mut conn, irregular, None).unwrap();
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
    let conn = migrations::open_database(&dir.path().join("main.sqlite3")).unwrap();
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
        INSERT INTO words(id,kanji,kana,meaning,search_text,created_at,updated_at)
        SELECT printf('00000000-0000-4000-8000-%012x',i),'働く','はたらく','工作','働く はたらく',1,i FROM n;",
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
