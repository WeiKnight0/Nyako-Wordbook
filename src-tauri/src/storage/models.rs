use serde::{Deserialize, Serialize};

/// @description 正式词库及当前成员数量。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub description: String,
    pub archived: bool,
    pub revision: i64,
    pub word_count: i64,
}

/// @description 带稳定身份的义项。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sense {
    pub id: String,
    pub meaning: String,
}

/// @description 可选关联义项的例句。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Example {
    pub id: String,
    pub sense_id: Option<String>,
    pub jp: String,
    pub cn: Option<String>,
}

/// @description 按拍数记录下降位置，零表示平板型。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pitch {
    pub id: String,
    pub mora_count: i64,
    pub downstep: i64,
    pub source: Option<String>,
}

/// @description 大小写敏感的归一化标签。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tag {
    pub id: String,
    pub name: String,
}

/// @description 一次原子保存的词条内容聚合。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WordInput {
    pub id: String,
    pub kanji: Option<String>,
    pub kana: String,
    pub jlpt: Option<String>,
    pub note: String,
    pub senses: Vec<Sense>,
    pub examples: Vec<Example>,
    pub pitches: Vec<Pitch>,
    pub pos: Vec<String>,
    pub tag_ids: Vec<String>,
    pub library_ids: Vec<String>,
}

/// @description 正式词条完整内容和并发版本。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordDetail {
    pub content: WordInput,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub card_state: Option<String>,
    pub due_at: Option<i64>,
    pub suspended: bool,
    pub buried_until: Option<i64>,
}

/// @description 虚拟列表所需的轻量词条投影。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordRow {
    pub id: String,
    pub kanji: Option<String>,
    pub kana: String,
    pub meaning: String,
    pub sense_count: i64,
    pub revision: i64,
    pub updated_at: i64,
    pub jlpt: Option<String>,
    pub pos: Vec<String>,
    pub tags: Vec<Tag>,
    pub card_state: Option<String>,
    pub due_at: Option<i64>,
    pub suspended: bool,
    pub buried_until: Option<i64>,
}

/// @description 正式检索的范围、过滤和排序条件。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordQuery {
    pub text: String,
    pub library_id: Option<String>,
    pub unclassified: bool,
    pub tag_ids: Vec<String>,
    pub tags_all: bool,
    pub jlpt: Option<String>,
    pub pos: Option<String>,
    pub state: Option<String>,
    pub due: Option<bool>,
    pub sort: String,
    pub descending: bool,
}

/// @description 带稳定游标和总数的正式检索页。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Serialize)]
pub struct WordPage {
    pub words: Vec<WordRow>,
    pub next_cursor: Option<String>,
    pub total: i64,
    pub query_id: String,
}

/// @description 显式选择对象及读取时的版本。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedWord {
    pub id: String,
    pub revision: i64,
}

/// @description 永久删除前的实际影响数量。
/// @param 不适用，数据契约。
/// @throws 不适用。
#[derive(Debug, Serialize)]
pub struct DeleteImpact {
    pub memberships: i64,
    pub cards: i64,
    pub events: i64,
}
