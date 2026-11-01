use super::models::WordInput;
use crate::error::AppError;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

pub(super) const POS: &[&str] = &[
    "noun",
    "godan_verb",
    "ichidan_verb",
    "suru_verb",
    "kuru_verb",
    "i_adjective",
    "na_adjective",
    "adverb",
    "particle",
    "expression",
    "other",
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

fn optional(value: Option<String>, max: usize) -> Result<Option<String>, AppError> {
    Ok(value
        .map(|v| text(&v, 0, max))
        .transpose()?
        .filter(|v| !v.is_empty()))
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

pub(super) fn mora_count(kana: &str) -> i64 {
    kana.chars()
        .filter(|c| !"ゃゅょぁぃぅぇぉャュョァィゥェォ".contains(*c) && !c.is_whitespace())
        .count() as i64
}

pub(super) fn word(mut value: WordInput) -> Result<WordInput, AppError> {
    id(&value.id)?;
    value.kanji = text(&value.kanji, 1, 1000)?;
    value.kana = text(&value.kana, 1, 1000)?;
    value.note = text(&value.note, 0, 100000)?;
    if value
        .jlpt
        .as_deref()
        .is_some_and(|v| !["N1", "N2", "N3", "N4", "N5"].contains(&v))
    {
        return Err(AppError::validation("JLPT 应为 N1—N5 或未知"));
    }
    if value.senses.is_empty() {
        return Err(AppError::validation("至少需要一个有效义项"));
    }
    unique(value.senses.iter().map(|v| v.id.as_str()), 100)?;
    unique(value.examples.iter().map(|v| v.id.as_str()), 100)?;
    unique(value.pitches.iter().map(|v| v.id.as_str()), 20)?;
    unique(value.library_ids.iter().map(String::as_str), 500)?;
    if value.pos.len() > POS.len()
        || value.pos.iter().any(|v| !POS.contains(&v.as_str()))
        || value.pos.iter().collect::<HashSet<_>>().len() != value.pos.len()
    {
        return Err(AppError::validation("词性编码无效或重复"));
    }
    for sense in &mut value.senses {
        sense.meaning = text(&sense.meaning, 1, 10000)?;
    }
    for example in &mut value.examples {
        example.jp = text(&example.jp, 1, 10000)?;
        example.cn = optional(example.cn.take(), 10000)?;
        if example
            .sense_id
            .as_ref()
            .is_some_and(|id| !value.senses.iter().any(|s| &s.id == id))
        {
            return Err(AppError::validation("例句关联的义项不属于当前词条"));
        }
    }
    if value
        .pitches
        .iter()
        .any(|pitch| !(0..=mora_count(&value.kana)).contains(&pitch.downstep))
    {
        return Err(AppError::validation("音调下降位置应在零与假名拍数之间"));
    }
    Ok(value)
}
