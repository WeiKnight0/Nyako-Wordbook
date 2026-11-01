use super::super::{models::*, validation};
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub kanji: String,
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
    pub note: String,
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
    pub downstep: i64,
}

pub(super) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl ExchangeWord {
    pub(super) fn input(&self) -> Result<WordInput, AppError> {
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
                    downstep: p.downstep,
                })
                .collect(),
            pos: self.parts_of_speech.clone(),
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
