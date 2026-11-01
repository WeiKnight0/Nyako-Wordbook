use super::{encode, format::*, hash, read_source, Preview};
use crate::{
    error::AppError,
    storage::{libraries::now, models::*, words},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Candidate {
    pub id: String,
    pub revision: i64,
    pub kanji: Option<String>,
    pub kana: String,
    pub meaning: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ImportRow {
    pub classification: String,
    pub locally_modified: bool,
    pub line: i64,
    pub content: Option<serde_json::Value>,
    pub error: Option<String>,
    pub candidates: Vec<Candidate>,
    pub matched: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Choice {
    pub line: i64,
    pub action: String,
    pub target: Option<String>,
    pub strategy: String,
    pub fields: HashSet<String>,
    pub clear: HashSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommitOptions {
    pub library_id: Option<String>,
    pub new_library: Option<String>,
    pub valid_only: bool,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct ImportResult {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

pub(super) fn stage(root: &Path, task: &str) -> Result<Connection, AppError> {
    crate::storage::validation::id(task)?;
    let path = root.join("staging").join(task).join("import.sqlite3");
    Ok(Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?)
}

fn metadata<T: serde::de::DeserializeOwned>(conn: &Connection, key: &str) -> Result<T, AppError> {
    let value: String =
        conn.query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
            row.get(0)
        })?;
    serde_json::from_str(&value).map_err(json_error)
}

fn changed() -> AppError {
    AppError::new("IMPORT_TARGET_CHANGED", "预览后目标数据已改变，请重新预览")
}

pub(super) fn snapshot(conn: &Connection) -> Result<String, AppError> {
    let mut result = sha2::Sha256::new();
    use sha2::Digest;
    for sql in ["SELECT id,revision FROM words ORDER BY id", "SELECT id,revision FROM libraries ORDER BY id", "SELECT library_id,word_id FROM library_words ORDER BY library_id,word_id", "SELECT source_namespace,external_id,word_id,last_import_hash FROM source_links ORDER BY source_namespace,external_id"] {
        let mut stmt = conn.prepare(sql)?;
        let columns = stmt.column_count();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            for column in 0..columns {
                let value = match row.get_ref(column)? { rusqlite::types::ValueRef::Text(value) => value.to_vec(), rusqlite::types::ValueRef::Integer(value) => value.to_le_bytes().to_vec(), _ => return Err(AppError::validation("身份快照包含无效数据")) };
                result.update((value.len() as u64).to_le_bytes());
                result.update(value);
            }
        }
    }
    Ok(result
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub(super) fn prepare(conn: &Connection, root: &Path, task: &str) -> Result<(), AppError> {
    let staged = stage(root, task)?;
    staged.execute(
        "INSERT OR REPLACE INTO metadata VALUES('target_snapshot',?1)",
        [encode(&snapshot(conn)?)?],
    )?;
    Ok(())
}

/// @description 列出已完整预览且尚未提交的暂存任务，不自动续写。
/// @param conn 正式库；root 数据根目录。
/// @throws 文件或暂存库读取失败时返回错误。
pub(super) fn pending(conn: &Connection, root: &Path) -> Result<Vec<Preview>, AppError> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(root.join("staging"))? {
        let path = entry?.path();
        if !path.join("import.sqlite3").is_file() {
            continue;
        }
        let task = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::validation("暂存目录名称无效"))?;
        let staged = stage(root, task)?;
        let ready = staged.query_row(
            "SELECT count(*) FROM metadata WHERE key IN ('preview','target_snapshot')",
            [],
            |row| row.get::<_, i64>(0),
        )? == 2;
        let committed = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id=?1 AND status='committed')",
            [task],
            |row| row.get::<_, bool>(0),
        )?;
        if ready && !committed {
            result.push(metadata(&staged, "preview")?);
        }
    }
    Ok(result)
}

/// @description 显式重新核对遗留任务，重新选择冲突策略后才可提交。
/// @param conn 正式库；root 数据根；task 已暂存任务标识。
/// @throws 源文件变动或读取错误时拒绝继续。
pub(super) fn resume(conn: &Connection, root: &Path, task: &str) -> Result<Preview, AppError> {
    let staged = stage(root, task)?;
    let preview: Preview = metadata(&staged, "preview")?;
    let source: PathBuf = metadata(&staged, "source")?;
    if hash(&read_source(&source)?) != preview.file_hash {
        return Err(AppError::new(
            "IMPORT_SOURCE_CHANGED",
            "源文件已变化，请重新预览",
        ));
    }
    prepare(conn, root, task)?;
    Ok(preview)
}

fn candidates(
    conn: &Connection,
    word: &ExchangeWord,
    namespace: &str,
) -> Result<(Option<String>, Vec<Candidate>), AppError> {
    let matched = match &word.external_id {
        Some(external) => conn
            .query_row(
                "SELECT word_id FROM source_links WHERE source_namespace=?1 AND external_id=?2",
                params![namespace, external],
                |r| r.get::<_, String>(0),
            )
            .optional()?,
        None => None,
    };
    let mut stmt = conn.prepare("SELECT w.id,w.revision,w.kanji,w.kana,(SELECT meaning FROM senses WHERE word_id=w.id ORDER BY position LIMIT 1) FROM words w WHERE w.id=?1 OR (w.kana=?2 AND coalesce(w.kanji,'')=?3) ORDER BY w.id")?;
    let mut rows: Vec<Candidate> = stmt
        .query_map(params![matched, word.kana, word.kanji], |r| {
            Ok(Candidate {
                id: r.get(0)?,
                revision: r.get(1)?,
                kanji: r.get(2)?,
                kana: r.get(3)?,
                meaning: r.get(4)?,
                content: serde_json::Value::Null,
            })
        })?
        .collect::<Result<_, _>>()?;
    rows.iter_mut().try_for_each(|candidate| {
        candidate.content =
            serde_json::to_value(portable(&words::get(conn, &candidate.id)?.content))
                .map_err(json_error)?;
        Ok::<_, AppError>(())
    })?;
    Ok((matched, rows))
}

pub(super) fn page(
    conn: &Connection,
    root: &Path,
    task: &str,
    offset: u32,
    errors: bool,
) -> Result<Vec<ImportRow>, AppError> {
    let stage = stage(root, task)?;
    let preview: Preview = metadata(&stage, "preview")?;
    let mut stmt = stage.prepare("SELECT line,content,error FROM rows WHERE (?1=0 OR error IS NOT NULL) ORDER BY line LIMIT 50 OFFSET ?2")?;
    let rows = stmt
        .query_map(params![errors, offset], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(line, content, error)| {
            let word = content
                .map(|text| serde_json::from_str::<ExchangeWord>(&text).map_err(json_error))
                .transpose()?;
            let (matched, candidates) = word
                .as_ref()
                .map(|word| candidates(conn, word, &preview.source_namespace))
                .transpose()?
                .unwrap_or_default();
            let mut comparable = word.clone();
            if let Some(word) = &mut comparable { word.external_id = None; }
            let incoming = comparable.as_ref().map(serde_json::to_value).transpose().map_err(json_error)?;
            let target = matched.as_ref().and_then(|id| candidates.iter().find(|candidate| &candidate.id == id));
            let same = candidates.iter().any(|candidate| Some(&candidate.content) == incoming.as_ref());
            let classification = if error.is_some() { "error" } else if candidates.is_empty() { "new" } else if matched.is_some() { if target.is_some_and(|candidate| Some(&candidate.content) == incoming.as_ref()) { "same" } else { "update" } } else if candidates.len() == 1 && same { "same" } else { "ambiguous" };
            let locally_modified = match (target, word.as_ref().and_then(|word| word.external_id.as_ref())) {
                (Some(target), Some(external)) => {
                    let previous: String = conn.query_row("SELECT last_import_hash FROM source_links WHERE source_namespace=?1 AND external_id=?2", params![preview.source_namespace, external], |row| row.get(0))?;
                    let content: ExchangeWord = serde_json::from_value(target.content.clone()).map_err(json_error)?;
                    !previous.is_empty() && previous != hash(encode(&content)?.as_bytes())
                }
                _ => false,
            };
            Ok(ImportRow {
                classification: classification.into(),
                locally_modified,
                line,
                content: word
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(json_error)?,
                error,
                candidates,
                matched,
            })
        })
        .collect()
}

/// @description 转换为不含数据库身份和学习状态的可交换内容。
/// @param input 完整词条。
/// @throws 不抛出。
pub(super) fn portable(input: &WordInput) -> ExchangeWord {
    ExchangeWord {
        external_id: None,
        kanji: input.kanji.clone(),
        kana: input.kana.clone(),
        parts_of_speech: input.pos.clone(),
        jlpt: input.jlpt.clone(),
        pitch_accents: input
            .pitches
            .iter()
            .map(|p| ExchangePitch {
                downstep: p.downstep,
            })
            .collect(),
        senses: input
            .senses
            .iter()
            .map(|s| ExchangeSense {
                meaning: s.meaning.clone(),
            })
            .collect(),
        examples: input
            .examples
            .iter()
            .map(|e| ExchangeExample {
                jp: e.jp.clone(),
                cn: e.cn.clone(),
                sense_index: e
                    .sense_id
                    .as_ref()
                    .and_then(|id| input.senses.iter().position(|s| &s.id == id)),
            })
            .collect(),
        note: input.note.clone(),
    }
}

fn empty(value: &serde_json::Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(str::is_empty)
        || value.as_array().is_some_and(Vec::is_empty)
}

fn merge(
    current: &ExchangeWord,
    incoming: &ExchangeWord,
    present: &HashSet<String>,
    choice: &Choice,
) -> Result<ExchangeWord, AppError> {
    if !["keep", "fill", "replace"].contains(&choice.strategy.as_str()) {
        return Err(AppError::validation("未知更新策略"));
    }
    let mut value = serde_json::to_value(current).map_err(json_error)?;
    let next = serde_json::to_value(incoming).map_err(json_error)?;
    for field in [
        "kanji",
        "kana",
        "parts_of_speech",
        "jlpt",
        "pitch_accents",
        "senses",
        "examples",
        "note",
    ] {
        if !present.contains(field) {
            continue;
        }
        let replace = choice.strategy == "replace" && choice.fields.contains(field);
        if !(replace || choice.strategy == "fill" && empty(&value[field])) {
            continue;
        }
        if empty(&next[field]) && !choice.clear.contains(field) {
            continue;
        }
        value[field] = next[field].clone();
    }
    let mut result: ExchangeWord = serde_json::from_value(value).map_err(json_error)?;
    result.validate()?;
    Ok(result)
}

/// @description 在单个正式库事务中合并已确认的暂存数据，保留学习状态。
/// @param conn 串行写连接；root 暂存根；task 预览身份；options 用户选择；cancel 独立取消令牌。
/// @throws 源或目标变化、冲突未解决、取消及数据库错误导致整批回滚。
pub(super) fn commit(
    conn: &mut Connection,
    root: &Path,
    task: &str,
    options: CommitOptions,
    control: &super::jobs::Control,
) -> Result<ImportResult, AppError> {
    let allowed: HashSet<_> = [
        "kanji",
        "kana",
        "parts_of_speech",
        "jlpt",
        "pitch_accents",
        "senses",
        "examples",
        "note",
    ]
    .into_iter()
    .collect();
    let mut lines = HashSet::new();
    if options.choices.iter().any(|choice| {
        !lines.insert(choice.line)
            || choice
                .fields
                .iter()
                .chain(&choice.clear)
                .any(|field| !allowed.contains(field.as_str()))
            || !choice.clear.is_subset(&choice.fields)
    }) {
        return Err(AppError::validation(
            "选择包含重复行、未知字段或未经选择的清空字段",
        ));
    }
    let stage = stage(root, task)?;
    let preview: Preview = metadata(&stage, "preview")?;
    let source: PathBuf = metadata(&stage, "source")?;
    if hash(&read_source(&source)?) != preview.file_hash {
        return Err(AppError::new(
            "IMPORT_SOURCE_CHANGED",
            "源文件已改变，请重新预览",
        ));
    }
    if preview.errors > 0 && !options.valid_only {
        return Err(AppError::validation("请修正错误，或明确选择仅导入有效行"));
    }
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(summary) = tx
        .query_row(
            "SELECT summary_json FROM import_jobs WHERE id=?1 AND status='committed'",
            [task],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        return serde_json::from_str(&summary).map_err(json_error);
    }
    if snapshot(&tx)? != metadata::<String>(&stage, "target_snapshot")? {
        return Err(changed());
    }
    let library = match (options.library_id, options.new_library) {
        (Some(id), None) => {
            if !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM libraries WHERE id=?1)",
                [&id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(changed());
            }
            id
        }
        (None, Some(name)) => {
            let name = crate::storage::validation::text(&name, 1, 100)?;
            let id = id();
            tx.execute(
                "INSERT INTO libraries(id,name,created_at,updated_at) VALUES(?1,?2,?3,?3)",
                params![id, name, now()?],
            )?;
            id
        }
        _ => return Err(AppError::validation("请选择一个目标词库或输入新词库名称")),
    };
    let mut result = ImportResult::default();
    let mut stmt = stage
        .prepare("SELECT line,content,present FROM rows WHERE content IS NOT NULL ORDER BY line")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if control.cancel.load(Ordering::Acquire) {
            return Err(AppError::new(
                "TASK_CANCELLED",
                "导入已取消，业务数据未改变",
            ));
        }
        control.processed.fetch_add(1, Ordering::Release);
        let line: i64 = row.get(0)?;
        let incoming: ExchangeWord =
            serde_json::from_str(&row.get::<_, String>(1)?).map_err(json_error)?;
        let present: HashSet<String> =
            serde_json::from_str(&row.get::<_, String>(2)?).map_err(json_error)?;
        let (matched, candidates) = candidates(&tx, &incoming, &preview.source_namespace)?;
        let default = Choice {
            line,
            action: if matched.is_some() {
                "reuse"
            } else if candidates.is_empty() {
                "new"
            } else {
                "unresolved"
            }
            .into(),
            target: matched,
            strategy: "keep".into(),
            fields: HashSet::new(),
            clear: HashSet::new(),
        };
        let choice = options
            .choices
            .iter()
            .find(|choice| choice.line == line)
            .unwrap_or(&default);
        if choice.action == "skip" {
            result.skipped += 1;
            continue;
        }
        let target = match choice.action.as_str() {
            "new" if default.target.is_none() => None,
            "reuse" => Some(
                choice
                    .target
                    .as_ref()
                    .filter(|id| candidates.iter().any(|c| &c.id == *id))
                    .ok_or_else(|| AppError::validation("请为复用选择有效候选"))?
                    .clone(),
            ),
            _ => return Err(AppError::validation("存在未解决的身份冲突")),
        };
        if default
            .target
            .as_ref()
            .is_some_and(|mapped| target.as_ref() != Some(mapped))
        {
            return Err(AppError::validation("已有来源映射只能复用原词条或跳过"));
        }
        let existing = target.as_ref().map(|id| words::get(&tx, id)).transpose()?;
        let current = existing.as_ref().map(|detail| portable(&detail.content));
        let mut merged = match &current {
            Some(current) => merge(current, &incoming, &present, choice)?,
            None => incoming.clone(),
        };
        merged.external_id = None;
        let unchanged = current.as_ref().is_some_and(|current| current == &merged);
        let word_id = existing
            .as_ref()
            .map(|v| v.content.id.clone())
            .unwrap_or_else(id);
        if !unchanged {
            let mut input = merged.input()?;
            input.id = word_id.clone();
            if let (Some(detail), Some(previous)) = (&existing, &current) {
                if previous.senses == merged.senses {
                    input.senses = detail.content.senses.clone();
                }
                if previous.examples == merged.examples
                    && previous.senses != merged.senses
                    && !previous.examples.is_empty()
                    && !(choice.strategy == "replace" && choice.fields.contains("examples"))
                {
                    return Err(AppError::validation("替换义项时请同时确认例句及其义项关联"));
                }
                input
                    .examples
                    .iter_mut()
                    .zip(&merged.examples)
                    .for_each(|(example, portable)| {
                        example.sense_id = portable
                            .sense_index
                            .map(|index| input.senses[index].id.clone());
                    });
                if previous.examples == merged.examples && previous.senses == merged.senses {
                    input.examples = detail.content.examples.clone();
                }
                if previous.pitch_accents == merged.pitch_accents {
                    input.pitches = detail.content.pitches.clone();
                }
            }
            input.library_ids = existing
                .as_ref()
                .map(|v| v.content.library_ids.clone())
                .unwrap_or_default();
            if !input.library_ids.contains(&library) {
                input.library_ids.push(library.clone());
            }
            words::save_in_transaction(&tx, input, existing.as_ref().map(|v| v.revision))?;
            if existing.is_some() {
                result.updated += 1;
            } else {
                result.created += 1;
            }
        } else {
            tx.execute(
                "INSERT OR IGNORE INTO library_words VALUES(?1,?2,?3)",
                params![library, word_id, now()?],
            )?;
            result.unchanged += 1;
        }
        if let Some(external) = &incoming.external_id {
            let mut imported = incoming.clone();
            imported.external_id = None;
            tx.execute("INSERT INTO source_links VALUES(?1,?2,?3,?4) ON CONFLICT(source_namespace,external_id) DO UPDATE SET last_import_hash=excluded.last_import_hash",params![preview.source_namespace,external,word_id,hash(encode(&imported)?.as_bytes())])?;
        }
    }
    if control.cancel.load(Ordering::Acquire) {
        return Err(AppError::new(
            "TASK_CANCELLED",
            "导入已取消，业务数据未改变",
        ));
    }
    tx.execute(
        "INSERT INTO import_jobs VALUES(?1,?2,?3,'committed',?4,?5,?5)",
        params![
            task,
            preview.file_hash,
            preview.format,
            encode(&result)?,
            now()?
        ],
    )?;
    control.publish(|| {
        tx.commit()?;
        Ok(())
    })?;
    Ok(result)
}
