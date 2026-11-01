use super::super::{models::*, validation};
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use unicode_normalization::UnicodeNormalization;

pub(super) const MAX_FILE: u64 = 256 * 1024 * 1024;
pub(super) const MAX_WORDS: usize = 500_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    pub format: String,
    pub version: u32,
    pub source_namespace: String,
    pub exported_at: String,
    pub words: Vec<ExchangeWord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ExchangeWord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(default)]
    pub kanji: Option<String>,
    pub kana: String,
    #[serde(default)]
    pub parts_of_speech: Vec<String>,
    #[serde(default)]
    pub jlpt: Option<String>,
    #[serde(default)]
    pub pitch_accents: Vec<ExchangePitch>,
    pub senses: Vec<ExchangeSense>,
    #[serde(default)]
    pub examples: Vec<ExchangeExample>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default = "plain")]
    pub note_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ExchangeSense {
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ExchangeExample {
    pub jp: String,
    #[serde(default)]
    pub cn: Option<String>,
    #[serde(default)]
    pub sense_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ExchangePitch {
    pub mora_count: i64,
    pub downstep: i64,
    #[serde(default)]
    pub source: Option<String>,
}

fn plain() -> String {
    "plain".into()
}

pub(super) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl ExchangeWord {
    pub(super) fn input(&self) -> Result<WordInput, AppError> {
        if self.note_format != "plain" {
            return Err(AppError::validation("M1 仅支持纯文本备注"));
        }
        let senses: Vec<_> = self
            .senses
            .iter()
            .map(|s| Sense {
                id: id(),
                meaning: s.meaning.clone(),
            })
            .collect();
        let examples = self
            .examples
            .iter()
            .map(|e| {
                let sense_id =
                    e.sense_index
                        .map(|index| {
                            senses.get(index).map(|s| s.id.clone()).ok_or_else(|| {
                                AppError::validation("例句 sense_index 超出义项范围")
                            })
                        })
                        .transpose()?;
                Ok(Example {
                    id: id(),
                    sense_id,
                    jp: e.jp.clone(),
                    cn: e.cn.clone(),
                })
            })
            .collect::<Result<_, AppError>>()?;
        validation::word(WordInput {
            id: id(),
            kanji: self.kanji.clone(),
            kana: self.kana.clone(),
            jlpt: self.jlpt.clone(),
            note: self.note.clone(),
            senses,
            examples,
            pitches: self
                .pitch_accents
                .iter()
                .map(|p| Pitch {
                    id: id(),
                    mora_count: p.mora_count,
                    downstep: p.downstep,
                    source: p.source.clone(),
                })
                .collect(),
            pos: self.parts_of_speech.clone(),
            tag_ids: vec![],
            library_ids: vec![],
        })
    }

    pub(super) fn validate(&mut self) -> Result<(), AppError> {
        let encoded = serde_json::to_value(&self).map_err(json_error)?;
        check_limits(&encoded, 0)?;
        if serde_json::to_vec(&self).map_err(json_error)?.len() > 1024 * 1024 {
            return Err(AppError::validation("单个词条超过 1MiB"));
        }
        let input = self.input()?;
        self.kanji = input.kanji;
        self.kana = input.kana;
        self.note = input.note;
        self.senses
            .iter_mut()
            .zip(input.senses)
            .for_each(|(out, value)| out.meaning = value.meaning);
        self.examples
            .iter_mut()
            .zip(input.examples)
            .for_each(|(out, value)| {
                out.jp = value.jp;
                out.cn = value.cn;
            });
        self.pitch_accents
            .iter_mut()
            .zip(input.pitches)
            .for_each(|(out, value)| out.source = value.source);
        if self.tags.len() > 100 {
            return Err(AppError::validation("每词最多 100 个标签"));
        }
        self.tags = self
            .tags
            .iter()
            .map(|tag| validation::text(tag, 1, 1000))
            .collect::<Result<_, _>>()?;
        self.tags.sort();
        self.tags.dedup();
        if let Some(external) = &self.external_id {
            validation::text(external, 1, 1000)?;
        }
        Ok(())
    }
}

pub(super) fn json_error(error: impl std::fmt::Display) -> AppError {
    AppError::new("INVALID_FORMAT", format!("文件格式无效：{error}"))
}

fn check_limits(value: &Value, depth: usize) -> Result<(), AppError> {
    if depth > 16 {
        return Err(AppError::validation("JSON 嵌套超过 16 层"));
    }
    match value {
        Value::String(s) if s.len() > 64 * 1024 => Err(AppError::validation("单个文本超过 64KiB")),
        Value::Array(values) => values.iter().try_for_each(|v| check_limits(v, depth + 1)),
        Value::Object(values) => values.values().try_for_each(|v| check_limits(v, depth + 1)),
        _ => Ok(()),
    }
}

struct StrictValue(Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("不含重复键的 JSON")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("非有限数值"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(StrictValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(StrictValue(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!("重复对象键：{key}")));
                    }
                    values.insert(key, map.next_value::<StrictValue>()?.0);
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

pub(super) fn strict_json(bytes: &[u8]) -> Result<Value, AppError> {
    let value = serde_json::from_slice::<StrictValue>(bytes)
        .map_err(json_error)?
        .0;
    check_limits(&value, 0)?;
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CsvOptions {
    pub encoding: String,
    pub delimiter: Option<char>,
    pub headers: bool,
    pub columns: BTreeMap<String, usize>,
    #[serde(default)]
    pub pos_mapping: BTreeMap<String, String>,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            encoding: "utf-8".into(),
            delimiter: None,
            headers: true,
            columns: BTreeMap::new(),
            pos_mapping: BTreeMap::new(),
        }
    }
}

pub(super) fn decode(bytes: &[u8], encoding: &str) -> Result<String, AppError> {
    match encoding {
        "utf-8" => std::str::from_utf8(bytes)
            .map(|s| s.trim_start_matches('\u{feff}').to_owned())
            .map_err(|_| {
                AppError::new(
                    "INVALID_ENCODING",
                    "文件不是有效 UTF-8，请明确选择 CP932 后重新预览",
                )
            }),
        "cp932" => encoding_rs::SHIFT_JIS
            .decode_without_bom_handling_and_without_replacement(bytes)
            .map(|s| s.into_owned())
            .ok_or_else(|| AppError::new("INVALID_ENCODING", "文件不是有效 CP932")),
        _ => Err(AppError::validation("不支持的字符编码")),
    }
}

pub(super) fn delimiter(text: &str, selected: Option<char>) -> Result<u8, AppError> {
    if let Some(value) = selected {
        return match value {
            ',' => Ok(b','),
            '\t' => Ok(b'\t'),
            ';' => Ok(b';'),
            _ => Err(AppError::validation("分隔符仅支持逗号、制表符或分号")),
        };
    }
    Ok((*b",\t;")
        .into_iter()
        .max_by_key(|separator| {
            let records: Vec<_> = csv::ReaderBuilder::new()
                .delimiter(*separator)
                .has_headers(false)
                .from_reader(text.as_bytes())
                .records()
                .take(20)
                .collect();
            let widths: Vec<_> = records
                .iter()
                .filter_map(|r| r.as_ref().ok().map(|r| r.len()))
                .collect();
            if widths.is_empty() || widths.iter().any(|width| *width != widths[0]) {
                return 0;
            }
            widths[0]
        })
        .unwrap_or(b','))
}

pub(super) const COLUMNS: &[&str] = &[
    "external_id",
    "kanji",
    "kana",
    "meaning",
    "part_of_speech",
    "pitch_accent",
    "mora_count",
    "jlpt",
    "example_jp",
    "example_cn",
    "tags",
    "note",
];

pub(super) fn csv_word(
    record: &csv::StringRecord,
    columns: &BTreeMap<String, usize>,
    options: &CsvOptions,
) -> Result<ExchangeWord, AppError> {
    let field = |name: &str| {
        columns
            .get(name)
            .and_then(|index| record.get(*index))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let optional = |name: &str| {
        let text = field(name);
        (!text.is_empty()).then_some(text)
    };
    let split = |name: &str| {
        field(name)
            .split('|')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().nfc().collect::<String>())
            .collect::<Vec<_>>()
    };
    let pos = split("part_of_speech")
        .into_iter()
        .map(|p| options.pos_mapping.get(&p).cloned().unwrap_or(p))
        .collect();
    let pitch_accents = match optional("pitch_accent") {
        None => vec![],
        Some(downstep) => vec![ExchangePitch {
            mora_count: field("mora_count")
                .parse()
                .map_err(|_| AppError::validation("音调需要正整数 mora_count"))?,
            downstep: downstep
                .parse()
                .map_err(|_| AppError::validation("pitch_accent 必须为非负整数"))?,
            source: None,
        }],
    };
    if optional("example_cn").is_some() && optional("example_jp").is_none() {
        return Err(AppError::validation("中文例句不能独立于日文例句"));
    }
    let examples = optional("example_jp")
        .map(|jp| ExchangeExample {
            jp,
            cn: optional("example_cn"),
            sense_index: None,
        })
        .into_iter()
        .collect();
    let mut value = ExchangeWord {
        external_id: optional("external_id"),
        kanji: optional("kanji"),
        kana: field("kana"),
        parts_of_speech: pos,
        jlpt: optional("jlpt").map(|s| format!("N{}", s.trim_start_matches(['N', 'n']))),
        pitch_accents,
        senses: vec![ExchangeSense {
            meaning: field("meaning"),
        }],
        examples,
        tags: split("tags"),
        note: field("note"),
        note_format: plain(),
    };
    value.validate()?;
    Ok(value)
}

pub(super) fn fields(columns: &BTreeMap<String, usize>) -> HashSet<String> {
    columns
        .keys()
        .map(|name| {
            match name.as_str() {
                "meaning" => "senses",
                "part_of_speech" => "parts_of_speech",
                "pitch_accent" | "mora_count" => "pitch_accents",
                "example_jp" | "example_cn" => "examples",
                other => other,
            }
            .to_string()
        })
        .collect()
}
