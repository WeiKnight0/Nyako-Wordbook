use super::{migrations, models::WordQuery, query, StorageService, VocabularyRequest};
use crate::error::AppError;
use rusqlite::Connection;
use serde_json::json;
use std::{path::Path, time::Instant};

fn fixture(conn: &mut Connection, count: u32) -> Result<(), AppError> {
    query::register(conn)?;
    let tx = conn.transaction()?;
    tx.execute_batch(&format!(
        "CREATE TEMP TABLE fixture(i INTEGER PRIMARY KEY, id TEXT NOT NULL);
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i+1<{count})
         INSERT INTO fixture SELECT i,printf('00000000-0000-4000-8000-%012x',i) FROM n;
         INSERT INTO libraries(id,name,created_at,updated_at) VALUES
         ('10000000-0000-4000-8000-000000000000','基准主库',1,1),
         ('10000000-0000-4000-8000-000000000001','共享词库',1,1);
         INSERT INTO tags(id,name) VALUES
         ('20000000-0000-4000-8000-000000000000','常用'),
         ('20000000-0000-4000-8000-000000000001','日语'),
         ('20000000-0000-4000-8000-000000000002','基准');
         INSERT INTO words(id,kanji,kana,jlpt,note,search_text,created_at,updated_at)
         SELECT id,CASE WHEN i%5<>0 THEN '仕事'||i END,'はたらく'||i,'N'||(i%5+1),
         CASE WHEN i%1000=0 THEN '罕见备注' ELSE '日常工作' END,
         nyako_normalize(coalesce(CASE WHEN i%5<>0 THEN '仕事'||i END,'')||char(10)||'はたらく'||i||char(10)||'工作学习'||i||char(10)||'劳动生活'||i||char(10)||CASE WHEN i%1000=0 THEN '罕见备注' ELSE '日常工作' END),1,i+1 FROM fixture;
         INSERT INTO senses(id,word_id,position,meaning)
         SELECT printf('30000000-0000-4000-8000-%012x',2*i),id,0,'工作学习'||i FROM fixture UNION ALL
         SELECT printf('30000000-0000-4000-8000-%012x',2*i+1),id,1,'劳动生活'||i FROM fixture;
         INSERT INTO examples(id,word_id,sense_id,position,jp,cn)
         SELECT printf('40000000-0000-4000-8000-%012x',2*i),id,printf('30000000-0000-4000-8000-%012x',2*i),0,'毎日働く','每天工作' FROM fixture UNION ALL
         SELECT printf('40000000-0000-4000-8000-%012x',2*i+1),id,printf('30000000-0000-4000-8000-%012x',2*i+1),1,'生活する','生活' FROM fixture;
         INSERT INTO word_pos SELECT id,'noun' FROM fixture;
         INSERT INTO word_tags SELECT f.id,t.id FROM fixture f CROSS JOIN tags t;
         INSERT INTO library_words SELECT '10000000-0000-4000-8000-000000000000',id,1 FROM fixture WHERE i%10<>0;
         INSERT INTO library_words SELECT '10000000-0000-4000-8000-000000000001',id,1 FROM fixture WHERE i%2=0;
         INSERT INTO cards(id,word_id,card_type,state,due_at,scheduler_version,parameter_set_id,eligible_since)
         SELECT printf('50000000-0000-4000-8000-%012x',i),id,'recognition',CASE WHEN i%3=0 THEN 'new' ELSE 'review' END,
         CASE i%3 WHEN 0 THEN NULL WHEN 1 THEN 1 ELSE 4102444800000 END,'6.6.2','fsrs6-default',1 FROM fixture;
         INSERT INTO sessions(id,scope_json,started_at,status,timezone,day_start_hour) VALUES('60000000-0000-4000-8000-000000000000','{{}}',1,'completed','UTC',4);
         INSERT INTO review_events(id,card_id,session_id,event_type,rating,reviewed_at,study_day,elapsed_ms,before_json,after_json,scheduler_version,parameter_set_id)
         SELECT printf('70000000-0000-4000-8000-%012x',5*f.i+v.n),printf('50000000-0000-4000-8000-%012x',f.i),'60000000-0000-4000-8000-000000000000','rating',3,v.n,'2026-01-01',1000,'{{}}','{{}}','6.6.2','fsrs6-default'
         FROM fixture f CROSS JOIN (SELECT 1 n UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5) v;
         DROP TABLE fixture;"
    ))?;
    tx.commit()?;
    conn.execute_batch("ANALYZE; PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

fn verify(conn: &Connection, count: u32) -> Result<(), AppError> {
    for (table, multiplier) in [
        ("words", 1),
        ("senses", 2),
        ("examples", 2),
        ("word_tags", 3),
        ("cards", 1),
        ("review_events", 5),
    ] {
        let actual: u32 =
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        if actual != count * multiplier {
            return Err(AppError::validation("基准夹具计数不匹配"));
        }
    }
    if conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? != "ok"
        || conn
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(AppError::validation("基准夹具完整性检查失败"));
    }
    for table in [
        "words",
        "senses",
        "examples",
        "cards",
        "sessions",
        "review_events",
    ] {
        let valid = conn
            .prepare(&format!("SELECT id FROM {table}"))?
            .query_map([], |row| row.get::<_, String>(0))?
            .all(|id| id.is_ok_and(|id| uuid::Uuid::parse_str(&id).is_ok()));
        if !valid {
            return Err(AppError::validation("夹具存在非法实体 UUID"));
        }
    }
    conn.execute(
        "INSERT INTO words_fts(words_fts,rank) VALUES('integrity-check',1)",
        [],
    )?;
    Ok(())
}

fn base() -> WordQuery {
    WordQuery {
        text: String::new(),
        library_id: None,
        unclassified: false,
        tag_ids: vec![],
        tags_all: true,
        jlpt: None,
        pos: None,
        state: None,
        due: None,
        sort: "updated_at".into(),
        descending: true,
    }
}

/// @description 在显式标记目录构造正式结构夹具，通过生产串行服务测量查询。
/// @param root 独立验收根目录；count 合成词数，正式验收为二十万。
/// @throws 返回标记、数据库、夹具一致性或业务查询错误。
pub fn run(root: &Path, count: u32) -> Result<(), AppError> {
    run_mode(root, count, "shared", false)
}

/// @description 为独立验收生成指定分布并按需测量生产查询。
/// @param root 标记数据根；count 词数；distribution shared 或 distributed；prepare_only 仅构造。
/// @throws 返回夹具、目录、数据库或查询错误。
pub fn run_mode(
    root: &Path,
    count: u32,
    distribution: &str,
    prepare_only: bool,
) -> Result<(), AppError> {
    if !["shared", "distributed"].contains(&distribution) {
        return Err(AppError::validation("未知标签分布"));
    }
    if !root.is_absolute() || !root.join(".nyako-acceptance").is_file() || count == 0 {
        return Err(AppError::validation(
            "基准必须使用绝对路径、验收标记及正数词数",
        ));
    }
    std::fs::create_dir_all(root.join("data"))?;
    let mut conn =
        migrations::open_database(&root.join("data/main.sqlite3"), &root.join("backups"))?;
    let existing: u32 = conn.query_row("SELECT count(*) FROM words", [], |r| r.get(0))?;
    let started = Instant::now();
    if existing == 0 {
        fixture(&mut conn, count)?;
        if distribution == "distributed" {
            conn.execute_batch("BEGIN;
                DELETE FROM word_tags;
                WITH RECURSIVE n(i) AS (SELECT 3 UNION ALL SELECT i+1 FROM n WHERE i<202)
                INSERT INTO tags SELECT printf('20000000-0000-4000-8000-%012x',i),'分布标签'||i FROM n;
                INSERT INTO word_tags SELECT id,'20000000-0000-4000-8000-000000000000' FROM words;
                INSERT INTO word_tags SELECT id,printf('20000000-0000-4000-8000-%012x',3+(rowid%100)) FROM words;
                INSERT INTO word_tags SELECT id,printf('20000000-0000-4000-8000-%012x',103+((rowid/100)%100)) FROM words;
                COMMIT; ANALYZE;")?;
        }
        std::fs::write(
            root.join(".nyako-fixture"),
            format!("v2:{count}:{distribution}"),
        )?;
    }
    if std::fs::read_to_string(root.join(".nyako-fixture"))? != format!("v2:{count}:{distribution}")
    {
        return Err(AppError::validation("目录包含不同分布的夹具"));
    }
    verify(&conn, count)?;
    if std::env::var_os("NYAKO_BENCHMARK_COMPARE").is_some() {
        for (name, sql) in [
            ("exists", "SELECT count(*) FROM words w WHERE EXISTS(SELECT 1 FROM word_tags t WHERE t.word_id=w.id AND t.tag_id=?1) AND EXISTS(SELECT 1 FROM word_tags t WHERE t.word_id=w.id AND t.tag_id=?2)"),
            ("intersection", "SELECT count(*) FROM words w WHERE w.id IN (SELECT word_id FROM word_tags WHERE tag_id=?1 INTERSECT SELECT word_id FROM word_tags WHERE tag_id=?2)"),
            ("group", "SELECT count(*) FROM words w WHERE w.id IN (SELECT word_id FROM word_tags WHERE tag_id IN (?1,?2) GROUP BY word_id HAVING count(*)=2)"),
        ] {
            let ids = ["20000000-0000-4000-8000-000000000000", if distribution == "shared" { "20000000-0000-4000-8000-000000000001" } else { "20000000-0000-4000-8000-000000000003" }];
            let mut times = Vec::new();
            for _ in 0..100 {
                let start = Instant::now();
                let _: i64 = conn.query_row(sql, ids, |row| row.get(0))?;
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            println!("{}", json!({"compare":name,"p50_ms":times[49],"p95_ms":times[94]}));
        }
    }
    println!(
        "{}",
        json!({"fixture_version":2,"distribution":distribution,"fixture_words":count,"events":count*5,"database_bytes":std::fs::metadata(root.join("data/main.sqlite3"))?.len(),"prepare_seconds":started.elapsed().as_secs_f64(),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"sqlite":rusqlite::version(),"durability":"WAL/FULL","samples":100,"cache":"OS file cache uncontrolled"})
    );
    drop(conn);
    if prepare_only {
        return Ok(());
    }
    let service = StorageService::start(root.to_path_buf())?;
    let mut cases = Vec::new();
    for sort in ["updated_at", "kanji", "kana", "due_at"] {
        let mut query = base();
        query.sort = sort.into();
        cases.push((format!("first/{sort}"), query.clone(), false));
        cases.push((format!("cursor/{sort}"), query, true));
    }
    for text in [
        "工",
        "工作",
        "ハタラク",
        "はたらく 工作",
        "罕见备注",
        "不存在的词",
    ] {
        let mut query = base();
        query.text = text.into();
        cases.push((format!("search/{text}"), query, false));
    }
    for all in [true, false] {
        let mut query = base();
        query.tag_ids = vec![
            "20000000-0000-4000-8000-000000000000".into(),
            if distribution == "distributed" {
                "20000000-0000-4000-8000-000000000003"
            } else {
                "20000000-0000-4000-8000-000000000001"
            }
            .into(),
        ];
        query.tags_all = all;
        cases.push((format!("tags/{all}"), query.clone(), false));
        query.library_id = Some("10000000-0000-4000-8000-000000000000".into());
        query.jlpt = Some("N5".into());
        query.due = Some(true);
        query.pos = Some("noun".into());
        cases.push((format!("compound/{all}"), query, false));
    }
    if distribution == "distributed" {
        for (name, ids) in [
            ("overlap", [3, 103]),
            ("disjoint", [3, 4]),
            ("missing", [3, 999]),
        ] {
            for all in [true, false] {
                let mut query = base();
                query.tag_ids = ids
                    .iter()
                    .map(|id| format!("20000000-0000-4000-8000-{id:012x}"))
                    .collect();
                query.tags_all = all;
                cases.push((format!("tags/{name}/{all}"), query, false));
            }
        }
    }
    let case_filter = std::env::var("NYAKO_BENCHMARK_CASE").unwrap_or_default();
    for (name, query, later) in cases
        .into_iter()
        .filter(|(name, _, _)| case_filter.is_empty() || name.starts_with(&case_filter))
    {
        let warm = service.vocabulary(VocabularyRequest::ListWords {
            query: query.clone(),
            cursor: None,
            limit: 100,
        })?;
        let cursor = if later {
            warm["next_cursor"].as_str().map(str::to_owned)
        } else {
            None
        };
        let mut times = Vec::new();
        for _ in 0..100 {
            let started = Instant::now();
            service.vocabulary(VocabularyRequest::ListWords {
                query: query.clone(),
                cursor: cursor.clone(),
                limit: 100,
            })?;
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{}",
            json!({"case":name,"total":warm["total"],"backend_p50_ms":times[49],"backend_p95_ms":times[94]})
        );
    }
    Ok(())
}
