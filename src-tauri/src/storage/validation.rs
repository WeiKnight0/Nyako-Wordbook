use super::models::WordInput;
use crate::error::AppError;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

/// 备注按 Unicode 字符计数；交换格式的字节限额须容纳每字符最多四个 UTF-8 字节。
pub(super) const MAX_NOTE_CHARS: usize = 100_000;

pub(super) const POS: &[&str] = &[
    "noun",
    "godan_verb",
    "ichidan_verb",
    "irregular_verb",
    "i_adjective",
    "na_adjective",
    "adverb",
    "particle",
    "expression",
    "other",
    "intransitive",
    "transitive",
];

pub(super) fn text(value: &str, min: usize, max: usize) -> Result<String, AppError> {
    let value: String = value.trim().nfc().collect();
    if !(min..=max).contains(&value.chars().count()) {
        return Err(AppError::validation(&format!(
            "文字长度应为 {min}—{max} 个字符"
        )));
    }
    Ok(value)
}

pub(super) fn id(value: &str) -> Result<(), AppError> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::validation("无效的实体标识"))
}

pub(super) fn unique<'a>(
    values: impl IntoIterator<Item = &'a str>,
    max: usize,
) -> Result<(), AppError> {
    let values: Vec<_> = values.into_iter().collect();
    if values.len() > max || values.iter().copied().collect::<HashSet<_>>().len() != values.len() {
        return Err(AppError::validation("集合数量超限或包含重复标识"));
    }
    values.into_iter().try_for_each(id)
}

pub(super) fn word(mut value: WordInput) -> Result<WordInput, AppError> {
    id(&value.id)?;
    value.kanji = text(&value.kanji, 1, 1000)?;
    value.kana = text(&value.kana, 1, 1000)?;
    value.note = text(&value.note, 0, MAX_NOTE_CHARS)?;
    if value
        .jlpt
        .as_deref()
        .is_some_and(|v| !["N1", "N2", "N3", "N4", "N5"].contains(&v))
    {
        return Err(AppError::validation("JLPT 应为 N1—N5 或未知"));
    }
    value.meaning = text(&value.meaning, 1, MAX_NOTE_CHARS)?;
    value.pitch = text(&value.pitch, 0, 1000)?;
    value.examples_jp = text(&value.examples_jp, 0, MAX_NOTE_CHARS)?;
    value.examples_cn = text(&value.examples_cn, 0, MAX_NOTE_CHARS)?;
    unique(value.library_ids.iter().map(String::as_str), 500)?;
    if value.pos.len() > POS.len()
        || value.pos.iter().any(|v| !POS.contains(&v.as_str()))
        || value.pos.iter().collect::<HashSet<_>>().len() != value.pos.len()
    {
        return Err(AppError::validation("词性编码无效或重复"));
    }
    if value.pos.contains(&"intransitive".to_string())
        && value.pos.contains(&"transitive".to_string())
    {
        return Err(AppError::validation("自动词和他动词不能同时标注"));
    }
    if value
        .pos
        .iter()
        .any(|pos| ["intransitive", "transitive"].contains(&pos.as_str()))
        && !value
            .pos
            .iter()
            .any(|pos| ["godan_verb", "ichidan_verb", "irregular_verb"].contains(&pos.as_str()))
    {
        return Err(AppError::validation("自动词或他动词需要搭配动词活用类别"));
    }
    Ok(value)
}
