use super::{encode, format::*, import::portable};
use crate::{error::AppError, storage::words};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::PathBuf};

/// @description 指定 Excel 或 JSON 可移植导出的范围。
/// @param 不适用，导出契约。
/// @throws 无效范围或输出变化时拒绝执行。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportOptions {
    pub path: PathBuf,
    pub format: String,
    pub library_id: Option<String>,
    pub selected: Vec<String>,
    #[serde(default)]
    pub expected: Option<super::output::Fingerprint>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExportResult {
    pub words: usize,
    pub format: String,
}

#[cfg(test)]
pub(super) fn export(
    conn: &mut Connection,
    options: ExportOptions,
) -> Result<ExportResult, AppError> {
    controlled(conn, options, &super::jobs::Control::default())
}

pub(super) fn controlled(
    conn: &mut Connection,
    options: ExportOptions,
    control: &super::jobs::Control,
) -> Result<ExportResult, AppError> {
    control.check()?;
    if !["json", "xlsx"].contains(&options.format.as_str()) {
        return Err(AppError::validation("导出支持 Excel 和 JSON"));
    }
    if options.library_id.is_some() == !options.selected.is_empty() {
        return Err(AppError::validation("请选择词库或一组词条"));
    }
    if super::output::fingerprint(&options.path)? != options.expected {
        return Err(AppError::new("OUTPUT_CHANGED", "目标已改变或未经覆盖确认"));
    }
    let parent = options
        .path
        .parent()
        .ok_or_else(|| AppError::validation("请选择完整输出路径"))?;
    let temporary = parent.join(format!(".nyako-export-{}.tmp", id()));
    let result = (|| {
        let tx = conn.transaction()?;
        let namespace: Option<String> = tx
            .query_row(
                "SELECT value_json FROM settings WHERE key='export_namespace'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let namespace = match namespace {
            Some(value) => serde_json::from_str::<String>(&value).map_err(json_error)?,
            None => {
                let value = id();
                tx.execute(
                    "INSERT INTO settings(key,value_json) VALUES('export_namespace',?1)",
                    [encode(&value)?],
                )?;
                value
            }
        };
        if let Some(library) = &options.library_id {
            if !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM libraries WHERE id=?1)",
                [library],
                |row| row.get::<_, bool>(0),
            )? {
                return Err(AppError::validation("目标词库不存在"));
            }
        }
        let ids = match &options.library_id {
            Some(library) => tx
                .prepare("SELECT word_id FROM library_words WHERE library_id=?1 ORDER BY word_id")?
                .query_map([library], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
            None => {
                crate::storage::validation::unique(
                    options.selected.iter().map(String::as_str),
                    500,
                )?;
                options.selected.clone()
            }
        };
        if options.format == "xlsx" {
            let mut content = Vec::new();
            for word_id in &ids {
                control.check()?;
                let mut word = portable(&words::get(&tx, word_id)?.content);
                let external = tx.query_row("SELECT external_id FROM source_links WHERE source_namespace=?1 AND word_id=?2 ORDER BY external_id LIMIT 1", params![namespace, word_id], |row| row.get::<_, String>(0)).optional()?.unwrap_or_else(id);
                tx.execute(
                    "INSERT OR IGNORE INTO source_links VALUES(?1,?2,?3,'')",
                    params![namespace, external, word_id],
                )?;
                word.external_id = Some(external);
                content.push(word);
            }
            super::excel::write(&temporary, &namespace, &content, control)?;
            OpenOptions::new()
                .write(true)
                .open(&temporary)?
                .sync_all()?;
            control.publish(|| {
                tx.commit()?;
                super::output::publish(&temporary, &options.path, options.expected.as_ref())
            })?;
            return Ok(ExportResult {
                words: ids.len(),
                format: "xlsx".into(),
            });
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let exported_at: String =
            tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
                row.get(0)
            })?;
        write!(output,"{{\"format\":\"nyako-wordbook\",\"version\":2,\"source_namespace\":{},\"exported_at\":{},\"words\":[",encode(&namespace)?,encode(&exported_at)?)?;
        for (index, word_id) in ids.iter().enumerate() {
            control.check()?;
            control
                .processed
                .store(index + 1, std::sync::atomic::Ordering::Release);
            let detail = words::get(&tx, word_id)?;
            let mut word = portable(&detail.content);
            let external=tx.query_row("SELECT external_id FROM source_links WHERE source_namespace=?1 AND word_id=?2 ORDER BY external_id LIMIT 1",params![namespace,word_id],|r|r.get::<_,String>(0)).optional()?.unwrap_or_else(id);
            tx.execute(
                "INSERT OR IGNORE INTO source_links VALUES(?1,?2,?3,'')",
                params![namespace, external, word_id],
            )?;
            word.external_id = Some(external);
            if index > 0 {
                output.write_all(b",")?;
            }
            output.write_all(encode(&word)?.as_bytes())?;
        }
        output.write_all(b"]}")?;
        output.sync_all()?;
        drop(output);
        control.check()?;
        let mut reader = serde_json::Deserializer::from_reader(std::fs::File::open(&temporary)?);
        serde::de::IgnoredAny::deserialize(&mut reader).map_err(json_error)?;
        reader.end().map_err(json_error)?;
        control.publish(|| {
            tx.commit()?;
            super::output::publish(&temporary, &options.path, options.expected.as_ref())?;
            Ok(())
        })?;
        Ok(ExportResult {
            words: ids.len(),
            format: options.format.clone(),
        })
    })();
    if temporary.exists() {
        std::fs::remove_file(&temporary)?;
    }
    result
}
