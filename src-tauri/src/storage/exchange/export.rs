use super::{encode, format::*, import::portable};
use crate::{error::AppError, storage::words};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::PathBuf};

/// @description 指定可移植导出的范围及已确认的 CSV 取舍。
/// @param 不适用，导出契约。
/// @throws 无效范围或未确认有损导出时拒绝执行。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportOptions {
    pub path: PathBuf,
    pub format: String,
    pub library_id: Option<String>,
    pub selected: Vec<String>,
    pub csv_loss_confirmed: bool,
    pub csv_original: bool,
    #[serde(default)]
    pub expected: Option<super::output::Fingerprint>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExportResult {
    pub words: usize,
    pub format: String,
}

fn safe_csv(value: &str, original: bool) -> String {
    if !original
        && (value.starts_with(['\t', '\r']) || value.trim_start().starts_with(['=', '+', '-', '@']))
    {
        return format!("'{value}");
    }
    value.into()
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
    if !["json", "csv"].contains(&options.format.as_str()) {
        return Err(AppError::validation("导出仅支持 JSON 和 CSV"));
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
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut csv = csv::WriterBuilder::new()
            .terminator(csv::Terminator::CRLF)
            .from_writer(output.try_clone()?);
        if options.format == "json" {
            let exported_at: String =
                tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
                    row.get(0)
                })?;
            write!(output,"{{\"format\":\"nyako-wordbook\",\"version\":1,\"source_namespace\":{},\"exported_at\":{},\"words\":[",encode(&namespace)?,encode(&exported_at)?)?;
        } else {
            if !options.csv_loss_confirmed {
                return Err(AppError::validation(
                    "CSV 仅保留首义项、首例句和首音调，请确认有损导出或选择 JSON",
                ));
            }
            output.write_all(b"\xef\xbb\xbf")?;
            csv.write_record(COLUMNS).map_err(json_error)?;
        }
        for (index, word_id) in ids.iter().enumerate() {
            control.check()?;
            control
                .processed
                .store(index + 1, std::sync::atomic::Ordering::Release);
            let detail = words::get(&tx, word_id)?;
            let mut word = portable(&tx, &detail.content)?;
            let external=tx.query_row("SELECT external_id FROM source_links WHERE source_namespace=?1 AND word_id=?2 ORDER BY external_id LIMIT 1",params![namespace,word_id],|r|r.get::<_,String>(0)).optional()?.unwrap_or_else(id);
            tx.execute(
                "INSERT OR IGNORE INTO source_links VALUES(?1,?2,?3,'')",
                params![namespace, external, word_id],
            )?;
            word.external_id = Some(external.clone());
            if options.format == "json" {
                if index > 0 {
                    output.write_all(b",")?;
                }
                output.write_all(encode(&word)?.as_bytes())?;
                continue;
            }
            if word.tags.iter().any(|tag| tag.contains('|')) {
                return Err(AppError::validation(
                    "标签包含 |，无法用 CSV 无歧义表示，请使用 JSON",
                ));
            }
            let pitch = word.pitch_accents.first();
            let example = word.examples.first();
            let row = [
                external,
                word.kanji.unwrap_or_default(),
                word.kana,
                word.senses
                    .first()
                    .map(|s| s.meaning.clone())
                    .unwrap_or_default(),
                word.parts_of_speech.join("|"),
                pitch.map(|p| p.downstep.to_string()).unwrap_or_default(),
                pitch.map(|p| p.mora_count.to_string()).unwrap_or_default(),
                word.jlpt.unwrap_or_default(),
                example.map(|e| e.jp.clone()).unwrap_or_default(),
                example.and_then(|e| e.cn.clone()).unwrap_or_default(),
                word.tags.join("|"),
                word.note,
            ];
            csv.write_record(
                row.iter()
                    .map(|value| safe_csv(value, options.csv_original)),
            )
            .map_err(json_error)?;
        }
        if options.format == "json" {
            output.write_all(b"]}")?;
        } else {
            csv.flush()?;
        }
        drop(csv);
        output.sync_all()?;
        drop(output);
        control.check()?;
        if options.format == "json" {
            let mut reader =
                serde_json::Deserializer::from_reader(std::fs::File::open(&temporary)?);
            serde::de::IgnoredAny::deserialize(&mut reader).map_err(json_error)?;
            reader.end().map_err(json_error)?;
        } else {
            let mut reader =
                csv::Reader::from_reader(std::io::BufReader::new(std::fs::File::open(&temporary)?));
            let count = reader.records().try_fold(0usize, |count, row| {
                control.check()?;
                row.map_err(json_error)?;
                Ok::<_, AppError>(count + 1)
            })?;
            if count != ids.len() {
                return Err(AppError::validation("CSV 输出行数不一致"));
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spreadsheet_formulas_are_guarded_without_changing_plain_values() {
        for value in ["=1+1", " +2", "-1", "@SUM(A1)", "\ttext", "\rtext"] {
            assert!(safe_csv(value, false).starts_with('\''));
        }
        assert_eq!(safe_csv("ねこ", false), "ねこ");
        assert_eq!(safe_csv("=1+1", true), "=1+1");
    }
}
