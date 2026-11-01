mod backup;
pub(crate) mod excel;
mod excel_template;
mod export;
mod format;
mod import;
pub(crate) mod jobs;
pub(crate) mod output;
mod report;
pub(crate) mod restore;
#[cfg(test)]
mod tests;

use import::CommitOptions;
use std::sync::atomic::Ordering;

/// @description 通过串行存储服务执行数据交换操作。
/// @param 不适用，命令契约。
/// @throws 执行错误由统一业务错误返回。
#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(crate) enum ExchangeRequest {
    PendingImports,
    ExcelTemplate {
        path: std::path::PathBuf,
        expected: Option<output::Fingerprint>,
    },
    ResumeImport {
        task_id: String,
    },
    Preview {
        path: std::path::PathBuf,
        options: ImportOptions,
    },
    InspectExcel {
        path: std::path::PathBuf,
        options: excel::ExcelOptions,
    },
    #[serde(skip)]
    PreparedPreview {
        preview: Preview,
    },
    Page {
        task_id: String,
        offset: u32,
        errors: bool,
    },
    Commit {
        task_id: String,
        options: CommitOptions,
    },
    Export {
        options: export::ExportOptions,
    },
    Backup {
        path: std::path::PathBuf,
        #[serde(default)]
        expected: Option<output::Fingerprint>,
    },
    ValidateBackup {
        path: std::path::PathBuf,
    },
    Restore {
        task_id: String,
    },
    ErrorReport {
        task_id: String,
        path: std::path::PathBuf,
        expected: Option<output::Fingerprint>,
    },
}

pub(super) fn execute(
    conn: &mut Connection,
    root: &Path,
    request: ExchangeRequest,
    control: &jobs::Control,
) -> Result<serde_json::Value, AppError> {
    control.check()?;
    let result = match request {
        ExchangeRequest::ExcelTemplate { path, expected } => {
            excel_template::save(&path, expected.as_ref(), control)?;
            Ok(serde_json::json!({"saved":true}))
        }
        ExchangeRequest::PendingImports => serde_json::to_value(import::pending(conn, root)?),
        ExchangeRequest::ResumeImport { task_id } => {
            serde_json::to_value(import::resume(conn, root, &task_id)?)
        }
        ExchangeRequest::Preview { .. } | ExchangeRequest::InspectExcel { .. } => {
            return Err(AppError::validation("预览必须先在独立文件任务中解析"))
        }
        ExchangeRequest::PreparedPreview { preview } => {
            import::prepare(conn, root, &preview.task_id)?;
            serde_json::to_value(preview)
        }
        ExchangeRequest::Page {
            task_id,
            offset,
            errors,
        } => serde_json::to_value(import::page(conn, root, &task_id, offset, errors)?),
        ExchangeRequest::Commit { task_id, options } => {
            serde_json::to_value(import::commit(conn, root, &task_id, options, control)?)
        }
        ExchangeRequest::Export { options } => {
            serde_json::to_value(export::controlled(conn, options, control)?)
        }
        ExchangeRequest::Backup { path, expected } => serde_json::to_value(backup::controlled(
            conn,
            root,
            &path,
            control,
            expected.as_ref(),
        )?),
        ExchangeRequest::ValidateBackup { path } => {
            let task_id = format::id();
            let directory = root.join("staging").join(&task_id).join("restore");
            let manifest = backup::validate_controlled(&path, &directory, control)?;
            Ok(serde_json::json!({"task_id":task_id,"manifest":manifest}))
        }
        ExchangeRequest::Restore { task_id } => {
            return control.publish(|| restore::schedule(conn, root, &task_id))
        }
        ExchangeRequest::ErrorReport {
            task_id,
            path,
            expected,
        } => {
            report::save(root, &task_id, &path, expected.as_ref(), control)?;
            Ok(serde_json::json!({"saved":true}))
        }
    };
    result.map_err(format::json_error)
}

use crate::error::AppError;
use format::{Document, ExchangeWord};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs::File, io::Read, path::Path};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportOptions {
    pub source_namespace: String,
    #[serde(default)]
    pub excel: excel::ExcelOptions,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Preview {
    pub task_id: String,
    pub format: String,
    pub source_namespace: String,
    pub file_hash: String,
    pub valid: usize,
    pub errors: usize,
    pub total: usize,
    pub columns: Vec<String>,
}

/// @description 在正式库队列外读取 Excel 工作表样本。
/// @param path Excel 文件；options 工作表规则；control 取消令牌。
/// @throws 文件或工作簿验证错误。
pub(super) fn inspect_excel(
    path: &Path,
    options: &excel::ExcelOptions,
    control: &jobs::Control,
) -> Result<serde_json::Value, AppError> {
    excel::inspect(&read_source(path)?, options, control)
}

fn encode(value: &impl Serialize) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(format::json_error)
}

fn read_source(path: &Path) -> Result<Vec<u8>, AppError> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > format::MAX_FILE {
        return Err(AppError::validation("请选择不超过 256MiB 的普通文件"));
    }
    let mut bytes = Vec::new();
    file.take(format::MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > format::MAX_FILE {
        return Err(AppError::validation("源文件超过 256MiB"));
    }
    Ok(bytes)
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn stage_row(
    conn: &Connection,
    row: usize,
    word: Result<ExchangeWord, AppError>,
    present: &HashSet<String>,
) -> Result<bool, AppError> {
    match word {
        Ok(word) => {
            conn.execute(
                "INSERT INTO rows(line,content,present) VALUES(?1,?2,?3)",
                params![row as i64, encode(&word)?, encode(present)?],
            )?;
            Ok(true)
        }
        Err(error) => {
            conn.execute(
                "INSERT INTO rows(line,error) VALUES(?1,?2)",
                params![row as i64, error.message],
            )?;
            Ok(false)
        }
    }
}

fn parse_json(
    conn: &Connection,
    bytes: &[u8],
    control: &jobs::Control,
) -> Result<(String, usize, usize), AppError> {
    let mut envelope = format::strict_json(bytes)?;
    // 词条的业务错误留在逐行预览；包络错误必须阻止整个文件进入提交阶段。
    let words = envelope
        .get_mut("words")
        .map(serde_json::Value::take)
        .and_then(|value| match value {
            serde_json::Value::Array(words) => Some(words),
            _ => None,
        })
        .ok_or_else(|| AppError::validation("JSON 缺少 words 数组"))?;
    if words.len() > format::MAX_WORDS {
        return Err(AppError::validation("词条超过 50 万条"));
    }
    envelope["words"] = serde_json::json!([]);
    let document: Document = serde_json::from_value(envelope).map_err(format::json_error)?;
    if document.format != "nyako-wordbook" || document.version != 2 {
        return Err(AppError::validation("不支持的 JSON 格式或版本"));
    }
    super::validation::text(&document.source_namespace, 1, 1000)?;
    let mut valid = 0;
    let mut identities = std::collections::HashMap::<&str, usize>::new();
    words
        .iter()
        .filter_map(|word| word.get("external_id").and_then(serde_json::Value::as_str))
        .for_each(|id| *identities.entry(id).or_default() += 1);
    for (index, value) in words.iter().enumerate() {
        control.check()?;
        control.processed.store(index + 1, Ordering::Release);
        let word =
            serde_json::from_value::<ExchangeWord>(value.clone())
                .map_err(format::json_error)
                .and_then(|mut word| {
                    word.validate()?;
                    if word.external_id.as_ref().is_some_and(|id| {
                        identities.get(id.as_str()).is_some_and(|count| *count > 1)
                    }) {
                        return Err(AppError::validation("文件内 external_id 重复"));
                    }
                    Ok(word)
                });
        let present = value
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        valid += usize::from(stage_row(conn, index + 1, word, &present)?);
    }
    Ok((document.source_namespace, valid, words.len()))
}

/// @description 将不可信导入文件解析到独立暂存数据库，不修改正式词库。
/// @param root 应用数据根目录；source 用户选定文件；options 编码和列映射。
/// @throws 返回文件访问、格式、限额或暂存数据库错误。
#[cfg(test)]
fn preview(root: &Path, source: &Path, options: &ImportOptions) -> Result<Preview, AppError> {
    preview_controlled(root, source, options, &jobs::Control::default())
}

/// @description 在调用方文件任务中解析暂存，避免占用正式库队列。
/// @param root 隔离数据根；source 输入文件；options 映射规则；control 取消和进度。
/// @throws 文件、格式或取消错误，不写正式库。
pub(super) fn preview_controlled(
    root: &Path,
    source: &Path,
    options: &ImportOptions,
    control: &jobs::Control,
) -> Result<Preview, AppError> {
    control.check()?;
    let bytes = read_source(source)?;
    control.check()?;
    let task_id = format::id();
    let directory = root.join("staging").join(&task_id);
    std::fs::create_dir_all(&directory)?;
    let mut conn = Connection::open(directory.join("import.sqlite3"))?;
    conn.execute_batch("CREATE TABLE rows(line INTEGER PRIMARY KEY,content TEXT,present TEXT,error TEXT); CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
    let tx = conn.transaction()?;
    let content = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    let is_json = content
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| byte == b'{');
    let is_excel = source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"));
    let (namespace, total) = if is_excel {
        let (namespace, rows, present) =
            excel::parse(&bytes, &options.excel, &options.source_namespace, control)?;
        let total = rows.len();
        for (line, word) in rows {
            stage_row(&tx, line, word, &present)?;
        }
        (namespace, total)
    } else if is_json {
        let (namespace, _, total) = parse_json(&tx, content, control)?;
        (namespace, total)
    } else {
        return Err(AppError::validation(
            "仅支持新版 Excel (.xlsx) 或 JSON 导入",
        ));
    };
    tx.execute("UPDATE rows SET content=NULL,error='文件内 external_id 重复' WHERE json_extract(content,'$.external_id') IN (SELECT json_extract(content,'$.external_id') FROM rows WHERE content IS NOT NULL AND json_extract(content,'$.external_id') IS NOT NULL GROUP BY json_extract(content,'$.external_id') HAVING count(*)>1)", [])?;
    let valid = tx.query_row(
        "SELECT count(*) FROM rows WHERE content IS NOT NULL",
        [],
        |row| row.get::<_, u32>(0),
    )? as usize;
    let preview = Preview {
        task_id,
        format: if is_excel { "xlsx" } else { "json" }.into(),
        source_namespace: namespace,
        file_hash: hash(&bytes),
        valid,
        errors: total - valid,
        total,
        columns: vec![],
    };
    for (key, value) in [
        ("preview", encode(&preview)?),
        ("source", encode(&source)?),
        ("options", encode(options)?),
    ] {
        tx.execute("INSERT INTO metadata VALUES(?1,?2)", params![key, value])?;
    }
    tx.commit()?;
    Ok(preview)
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn json_rejects_duplicate_keys_unknown_fields_and_bad_relations() {
        assert!(format::strict_json(br#"{"a":1,"a":2}"#).is_err());
        let mut word: ExchangeWord = serde_json::from_value(serde_json::json!({"kanji":"猫","kana":"ねこ","senses":[{"meaning":"猫"}],"examples":[{"jp":"猫だ","sense_index":2}]})).unwrap();
        assert!(word.validate().is_err());
        assert!(serde_json::from_value::<ExchangeWord>(
            serde_json::json!({"kana":"ねこ","senses":[],"card_state":"new"})
        )
        .is_err());
    }

    #[test]
    fn csv_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("words.csv");
        std::fs::write(&file, "写法,假名,释义\n猫,ねこ,猫").unwrap();
        assert!(preview(
            root.path(),
            &file,
            &ImportOptions {
                source_namespace: "test".into(),
                excel: Default::default(),
            }
        )
        .is_err());
    }
}
