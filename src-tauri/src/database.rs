use crate::error::AppError;
use rusqlite::{params, params_from_iter, Connection};
use serde::Serialize;
use std::{path::Path, time::Instant};
use unicode_normalization::UnicodeNormalization;

/// @description 合成检索词条的展示内容。
/// @param 不适用，数据类型。
/// @throws 不适用。
#[derive(Debug, Serialize)]
pub struct Word {
    pub id: i64,
    pub kanji: String,
    pub kana: String,
    pub meaning: String,
}
/// @description 检索结果及后续分页位置。
/// @param 不适用，数据类型。
/// @throws 不适用。
#[derive(Debug, Serialize)]
pub struct SearchPage {
    pub words: Vec<Word>,
    pub next_cursor: Option<i64>,
    pub elapsed_ms: f64,
}
/// @description 合成数据的持久化、约束及调度验证报告。
/// @param 不适用，数据类型。
/// @throws 不适用。
#[derive(Serialize)]
pub struct Diagnostics {
    pub journal_mode: String,
    pub foreign_keys: bool,
    pub synchronous: i64,
    pub busy_timeout: i64,
    pub fts5: bool,
    pub rollback: bool,
    pub reopened: bool,
    pub same_day_fsrs: bool,
    pub word_count: i64,
}

/// @description 统一 Unicode、大小写与片假名检索形式。
/// @param text 待索引或查询的文字。
/// @throws 无。
pub fn normalize(text: &str) -> String {
    text.nfc()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if ('\u{30a1}'..='\u{30f6}').contains(&c) {
                char::from_u32(c as u32 - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

fn open(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

/// @description 首次创建独立的合成检索数据，已有数据保持不变。
/// @param path 验证数据库路径；count 首次生成的合成词条数。
/// @throws 返回数据库创建或写入错误。
pub fn initialize(path: &Path, count: usize) -> Result<(), AppError> {
    let mut conn = open(path)?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS probe_words(id INTEGER PRIMARY KEY, kanji TEXT NOT NULL, kana TEXT NOT NULL, meaning TEXT NOT NULL, search_text TEXT NOT NULL);
        CREATE VIRTUAL TABLE IF NOT EXISTS probe_fts USING fts5(search_text, tokenize='trigram');")?;
    let existing: i64 = conn.query_row("SELECT count(*) FROM probe_words", [], |r| r.get(0))?;
    if existing > 0 {
        return Ok(());
    }
    let tx = conn.transaction()?;
    {
        let mut word = tx.prepare("INSERT INTO probe_words VALUES (?1,?2,?3,?4,?5)")?;
        let mut fts = tx.prepare("INSERT INTO probe_fts(rowid,search_text) VALUES (?1,?2)")?;
        let samples = [
            ("働く", "はたらく", "工作；劳动"),
            ("学校", "がっこう", "学校；学习场所"),
            ("猫", "ねこ", "猫"),
            ("食べる", "たべる", "吃；进食"),
            ("コーヒー", "コーヒー", "咖啡"),
        ];
        for i in 0..count {
            let (kanji, kana, meaning) = samples[i % samples.len()];
            let display = if i < samples.len() {
                kanji.into()
            } else {
                format!("{kanji}・検証{i:06}")
            };
            let text = normalize(&format!("{display} {kana} {meaning}"));
            word.execute(params![i as i64 + 1, display, kana, meaning, text])?;
            fts.execute(params![i as i64 + 1, text])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// @description 按规范化关键词交集检索合成词条，并以标识分页。
/// @param path 验证库路径；query 用户关键词；after 上页末尾标识；limit 页大小。
/// @throws 返回输入校验或数据库查询错误。
pub fn search(path: &Path, query: &str, after: i64, limit: u32) -> Result<SearchPage, AppError> {
    if query.chars().count() > 200 || after < 0 || !(1..=500).contains(&limit) {
        return Err(AppError::validation(
            "查询长度上限 200 字，分页大小应为 1—500",
        ));
    }
    let start = Instant::now();
    let conn = open(path)?;
    let terms: Vec<String> = query.split_whitespace().map(normalize).collect();
    let indexed: Vec<String> = terms
        .iter()
        .filter(|t| t.chars().count() >= 3)
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    let mut sql = "SELECT id, kanji, kana, meaning FROM probe_words WHERE id > ?".to_string();
    let mut values: Vec<rusqlite::types::Value> = vec![after.into()];
    if !indexed.is_empty() {
        sql.push_str(" AND id IN (SELECT rowid FROM probe_fts WHERE probe_fts MATCH ?)");
        values.push(indexed.join(" AND ").into());
    }
    for term in terms {
        sql.push_str(" AND instr(search_text, ?) > 0");
        values.push(term.into());
    }
    sql.push_str(" ORDER BY id LIMIT ?");
    values.push((i64::from(limit) + 1).into());
    let mut stmt = conn.prepare(&sql)?;
    let mut words = stmt
        .query_map(params_from_iter(values), |r| {
            Ok(Word {
                id: r.get(0)?,
                kanji: r.get(1)?,
                kana: r.get(2)?,
                meaning: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if words.len() > limit as usize {
        words.pop();
        words.last().map(|w| w.id)
    } else {
        None
    };
    Ok(SearchPage {
        words,
        next_cursor,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// @description 实测验证库的约束、回滚、重开与调度能力。
/// @param path 独立的合成验证数据库路径。
/// @throws 返回数据库或调度验证错误。
pub fn diagnostics(path: &Path) -> Result<Diagnostics, AppError> {
    let conn = open(path)?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS probe_parent(id INTEGER PRIMARY KEY); CREATE TABLE IF NOT EXISTS probe_child(parent_id INTEGER REFERENCES probe_parent(id)); BEGIN; INSERT INTO probe_parent DEFAULT VALUES; ROLLBACK;")?;
    let rollback = conn.query_row("SELECT count(*)=0 FROM probe_parent", [], |r| r.get(0))?;
    let foreign_keys = conn
        .execute("INSERT INTO probe_child VALUES (-1)", [])
        .is_err();
    let report = Diagnostics {
        journal_mode: conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?,
        foreign_keys,
        synchronous: conn.query_row("PRAGMA synchronous", [], |r| r.get(0))?,
        busy_timeout: conn.query_row("PRAGMA busy_timeout", [], |r| r.get(0))?,
        fts5: conn.query_row("SELECT count(*)>=0 FROM probe_fts", [], |r| r.get(0))?,
        rollback,
        reopened: false,
        same_day_fsrs: crate::scheduler::verify_same_day()?,
        word_count: conn.query_row("SELECT count(*) FROM probe_words", [], |r| r.get(0))?,
    };
    drop(conn);
    let reopened = open(path)?.query_row("SELECT count(*) FROM probe_words", [], |r| {
        r.get::<_, i64>(0)
    })? == report.word_count;
    Ok(Diagnostics { reopened, ..report })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_constraints_and_search_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("probe.sqlite3");
        initialize(&path, 25).unwrap();
        let d = diagnostics(&path).unwrap();
        assert!(d.foreign_keys && d.rollback && d.reopened && d.fts5);
        assert_eq!(d.journal_mode, "wal");
        assert_eq!(d.synchronous, 2);
        assert_eq!(d.busy_timeout, 5000);
        for q in ["はたらく", "ハタラク", "工作", "工", "働く 労動"] {
            let result = search(&path, q, 0, 100).unwrap();
            if q.contains("労動") {
                assert!(result.words.is_empty());
            } else {
                assert!(!result.words.is_empty());
            }
        }
        assert!(!search(&path, "はたらく 工作", 0, 100)
            .unwrap()
            .words
            .is_empty());
        assert_eq!(normalize("カ\u{3099} ABC"), "が abc");
        let page = search(&path, "", 0, 10).unwrap();
        assert_eq!(page.words.len(), 10);
        let next = search(&path, "", page.next_cursor.unwrap(), 10).unwrap();
        assert!(next.words[0].id > page.words[9].id);
        assert!(search(&path, "", 0, 501).is_err());
        initialize(&path, 25).unwrap();
        assert_eq!(diagnostics(&path).unwrap().word_count, 25);
    }
}
