use super::{libraries::now, models::*, validation};
use crate::{database::normalize, error::AppError};
use rusqlite::{params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
struct Cursor {
    query: String,
    value: ValueKey,
    id: String,
    at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ValueKey {
    Text(String),
    Number(i64),
    Null,
}

pub(super) fn register(conn: &Connection) -> Result<(), AppError> {
    conn.create_scalar_function(
        "nyako_normalize",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(normalize(&ctx.get::<String>(0)?)),
    )?;
    Ok(())
}

pub(super) fn list(
    conn: &Connection,
    query: WordQuery,
    cursor: Option<String>,
    limit: u32,
) -> Result<WordPage, AppError> {
    if query.text.chars().count() > 200 || !(1..=500).contains(&limit) || query.tag_ids.len() > 100
    {
        return Err(AppError::validation("检索条件或分页大小超限"));
    }
    if query.library_id.is_some() && query.unclassified {
        return Err(AppError::validation("词库和未分类范围不能同时使用"));
    }
    if let Some(id) = &query.library_id {
        validation::id(id)?;
    }
    validation::unique(query.tag_ids.iter().map(String::as_str), 100)?;
    if query
        .jlpt
        .as_deref()
        .is_some_and(|v| !["N1", "N2", "N3", "N4", "N5"].contains(&v))
        || query
            .pos
            .as_deref()
            .is_some_and(|v| !validation::POS.contains(&v))
        || query
            .state
            .as_deref()
            .is_some_and(|v| !["new", "learning", "review", "relearning"].contains(&v))
    {
        return Err(AppError::validation("无效筛选条件"));
    }
    let column = match query.sort.as_str() {
        "kanji" => "coalesce(w.kanji,'')",
        "kana" => "w.kana",
        "updated_at" => "w.updated_at",
        "due_at" => "c.due_at",
        _ => return Err(AppError::validation("无效排序字段")),
    };
    let query_id = Sha256::digest(
        serde_json::to_vec(&query).map_err(|_| AppError::validation("查询不可序列化"))?,
    )
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect::<String>();
    let cursor = cursor
        .map(|c| {
            serde_json::from_str::<Cursor>(&c).map_err(|_| AppError::validation("无效分页游标"))
        })
        .transpose()?;
    if cursor.as_ref().is_some_and(|c| c.query != query_id) {
        return Err(AppError::validation("分页游标与查询不匹配"));
    }
    if let Some(c) = &cursor {
        validation::id(&c.id)?;
        match (&c.value, query.sort.as_str()) {
            (ValueKey::Text(_), "kanji" | "kana")
            | (ValueKey::Number(_), "updated_at" | "due_at")
            | (ValueKey::Null, "due_at") => {}
            _ => return Err(AppError::validation("分页排序值类型无效")),
        }
    }
    let at = cursor.as_ref().map(|c| c.at).unwrap_or(now()?);
    let mut conditions = vec!["1=1".to_string()];
    let mut values: Vec<Value> = vec![];
    if let Some(id) = &query.library_id {
        conditions.push(
            "EXISTS(SELECT 1 FROM library_words m WHERE m.word_id=w.id AND m.library_id=?)".into(),
        );
        values.push(id.clone().into());
    }
    if query.unclassified {
        conditions.push("NOT EXISTS(SELECT 1 FROM library_words m WHERE m.word_id=w.id)".into());
    }
    for (value, sql) in [
        (&query.jlpt, "w.jlpt=?"),
        (
            &query.pos,
            "EXISTS(SELECT 1 FROM word_pos p WHERE p.word_id=w.id AND p.pos_code=?)",
        ),
        (&query.state, "c.state=?"),
    ] {
        if let Some(v) = value {
            conditions.push(sql.into());
            values.push(v.clone().into());
        }
    }
    if let Some(due) = query.due {
        conditions.push(
            if due {
                "c.due_at IS NOT NULL AND c.due_at<=?"
            } else {
                "(c.due_at IS NULL OR c.due_at>?)"
            }
            .into(),
        );
        values.push(at.into());
    }
    if !query.tag_ids.is_empty() {
        let placeholders = std::iter::repeat_n("?", query.tag_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let counts = query
            .tag_ids
            .iter()
            .map(|id| {
                conn.query_row(
                    "SELECT count(*) FROM (SELECT 1 FROM word_tags WHERE tag_id=? LIMIT 10000)",
                    [id],
                    |r| r.get::<_, i64>(0),
                )
                .map(|count| (id, count))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some((id, _)) = counts
            .iter()
            .min_by_key(|(_, count)| *count)
            .filter(|(_, count)| query.tags_all && *count < 10_000)
        {
            conditions.push("w.id IN (SELECT word_id FROM word_tags WHERE tag_id=?)".into());
            values.push((*id).clone().into());
        }
        conditions.push(if query.tags_all {
            std::iter::repeat_n("EXISTS(SELECT 1 FROM word_tags t WHERE t.word_id=w.id AND t.tag_id=?)", query.tag_ids.len()).collect::<Vec<_>>().join(" AND ")
        } else if counts.iter().all(|(_, count)| *count < 10_000) {
            format!("w.id IN (SELECT word_id FROM word_tags WHERE tag_id IN ({placeholders}))")
        } else {
            format!("EXISTS(SELECT 1 FROM word_tags t WHERE t.word_id=w.id AND t.tag_id IN ({placeholders}))")
        });
        values.extend(query.tag_ids.iter().cloned().map(Value::Text));
    }
    let terms = query
        .text
        .split_whitespace()
        .map(normalize)
        .collect::<Vec<_>>();
    let indexed = terms
        .iter()
        .filter(|s| s.chars().count() >= 3)
        .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    let fts_position = if indexed.is_empty() {
        None
    } else {
        let position = (conditions.len(), values.len());
        conditions.push("w.rowid IN (SELECT rowid FROM words_fts WHERE words_fts MATCH ?)".into());
        values.push(indexed.join(" AND ").into());
        Some(position)
    };
    for term in terms {
        conditions.push("instr(w.search_text,?)>0".into());
        values.push(Value::Text(term));
    }
    let base = format!(
        "FROM words w LEFT JOIN cards c ON c.word_id=w.id AND c.card_type='recognition' WHERE {}",
        conditions.join(" AND ")
    );
    let count_base = if query.state.is_some() || query.due.is_some() {
        base.clone()
    } else {
        format!("FROM words w WHERE {}", conditions.join(" AND "))
    };
    #[cfg(feature = "benchmark")]
    if std::env::var_os("NYAKO_BENCHMARK_EXPLAIN").is_some() {
        let plan = conn
            .prepare(&format!("EXPLAIN QUERY PLAN SELECT count(*) {count_base}"))?
            .query_map(params_from_iter(values.iter()), |row| {
                row.get::<_, String>(3)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        eprintln!("COUNT: {}", plan.join("; "));
    }
    let total = conn.query_row(
        &format!("SELECT count(*) {count_base}"),
        params_from_iter(values.iter()),
        |r| r.get(0),
    )?;
    if let Some((condition, parameter)) = fts_position.filter(|_| total > 10_000) {
        conditions.remove(condition);
        values.remove(parameter);
    }
    let base = format!(
        "FROM words w LEFT JOIN cards c ON c.word_id=w.id AND c.card_type='recognition' WHERE {}",
        conditions.join(" AND ")
    );
    let mut paging = String::new();
    if let Some(c) = &cursor {
        let op = if query.descending { "<" } else { ">" };
        match &c.value {
            ValueKey::Null => {
                paging = format!(" AND ({column} IS NULL AND w.id>?)");
                values.push(c.id.clone().into());
            }
            value => {
                paging = format!(
                    " AND (({column} {op} ? OR ({column}=? AND w.id>?)){} )",
                    if query.sort == "due_at" {
                        format!(" OR {column} IS NULL")
                    } else {
                        String::new()
                    }
                );
                let value = match value {
                    ValueKey::Text(v) => Value::Text(v.clone()),
                    ValueKey::Number(v) => Value::Integer(*v),
                    ValueKey::Null => unreachable!(),
                };
                values.extend([value.clone(), value, c.id.clone().into()]);
            }
        }
    }
    let null_order = if query.sort == "due_at" {
        format!("{column} IS NULL ASC,")
    } else {
        String::new()
    };
    let direction = if query.descending { "DESC" } else { "ASC" };
    let sql = format!(
        "WITH page AS MATERIALIZED (
            SELECT w.id,{column} AS sort_key {base}{paging}
            ORDER BY {null_order}{column} {direction},w.id ASC LIMIT ?
        )
        SELECT w.id,w.kanji,w.kana,
            (SELECT meaning FROM senses s WHERE s.word_id=w.id ORDER BY position,id LIMIT 1),
            (SELECT count(*) FROM senses s WHERE s.word_id=w.id),
            w.revision,w.updated_at,w.jlpt,c.state,c.due_at,coalesce(c.suspended,0),c.buried_until
        FROM page p JOIN words w ON w.id=p.id
        LEFT JOIN cards c ON c.word_id=w.id AND c.card_type='recognition'
        ORDER BY p.sort_key IS NULL ASC,p.sort_key {direction},p.id ASC"
    );
    values.push((i64::from(limit) + 1).into());
    #[cfg(feature = "benchmark")]
    if std::env::var_os("NYAKO_BENCHMARK_EXPLAIN").is_some() {
        let plan = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map(params_from_iter(values.iter()), |row| {
                row.get::<_, String>(3)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        eprintln!("{}", plan.join("\n"));
    }
    let mut rows = conn
        .prepare(&sql)?
        .query_map(params_from_iter(values), |r| {
            Ok(WordRow {
                id: r.get(0)?,
                kanji: r.get(1)?,
                kana: r.get(2)?,
                meaning: r.get(3)?,
                sense_count: r.get(4)?,
                revision: r.get(5)?,
                updated_at: r.get(6)?,
                jlpt: r.get(7)?,
                pos: vec![],
                tags: vec![],
                card_state: r.get(8)?,
                due_at: r.get(9)?,
                suspended: r.get(10)?,
                buried_until: r.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let more = rows.len() > limit as usize;
    if more {
        rows.pop();
    }
    for row in &mut rows {
        row.pos = conn
            .prepare("SELECT pos_code FROM word_pos WHERE word_id=?1 ORDER BY pos_code")?
            .query_map([&row.id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        row.tags=conn.prepare("SELECT t.id,t.name FROM tags t JOIN word_tags m ON m.tag_id=t.id WHERE m.word_id=?1 ORDER BY t.name,t.id")?.query_map([&row.id],|r|Ok(Tag{id:r.get(0)?,name:r.get(1)?}))?.collect::<Result<_,_>>()?;
    }
    let next_cursor = if more {
        rows.last()
            .map(|r| {
                serde_json::to_string(&Cursor {
                    query: query_id.clone(),
                    value: match query.sort.as_str() {
                        "kanji" => ValueKey::Text(r.kanji.clone().unwrap_or_default()),
                        "kana" => ValueKey::Text(r.kana.clone()),
                        "due_at" => r.due_at.map(ValueKey::Number).unwrap_or(ValueKey::Null),
                        _ => ValueKey::Number(r.updated_at),
                    },
                    id: r.id.clone(),
                    at,
                })
            })
            .transpose()
            .map_err(|_| AppError::validation("游标不可序列化"))?
    } else {
        None
    };
    Ok(WordPage {
        words: rows,
        next_cursor,
        total,
        query_id,
    })
}
