use super::{format::*, jobs::Control};
use crate::error::AppError;
use calamine::{Data, Reader, Xlsx};
use rust_xlsxwriter::{Format, Workbook};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::Cursor,
    path::Path,
};

pub(super) const LABELS: &[&str] = &[
    "词条标识",
    "写法",
    "假名",
    "释义",
    "词性",
    "音调",
    "JLPT",
    "日文例句",
    "例句中文翻译",
    "备注",
];

/// @description 接收单表 Excel 的兼容参数，拒绝旧列映射以免误读。 @param sheet 可选工作表名；headers 表头标志；columns 旧列映射。 @throws 旧映射或缺表头在预览时拒绝。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExcelOptions {
    pub sheet: Option<String>,
    #[serde(default = "yes")]
    pub headers: bool,
    #[serde(default)]
    pub columns: BTreeMap<String, usize>,
    #[serde(default)]
    pub pos_mapping: BTreeMap<String, String>,
}
impl Default for ExcelOptions {
    fn default() -> Self {
        Self {
            sheet: None,
            headers: true,
            columns: BTreeMap::new(),
            pos_mapping: BTreeMap::new(),
        }
    }
}
fn yes() -> bool {
    true
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::validation(&message.into())
}
fn cell(value: &Data) -> Result<String, AppError> {
    match value {
        Data::Empty => Ok(String::new()),
        Data::String(value) => Ok(value.clone()),
        Data::Int(value) => Ok(value.to_string()),
        Data::Float(value) if value.is_finite() => Ok(value.to_string()),
        _ => Err(invalid("日期、布尔值和错误单元格不能作为词条内容")),
    }
}
fn open<'a>(bytes: &'a [u8], control: &Control) -> Result<Xlsx<Cursor<&'a [u8]>>, AppError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(json_error)?;
    if zip.len() > 100_000 {
        return Err(invalid("Excel 文件条目过多"));
    }
    let mut total = 0u64;
    for index in 0..zip.len() {
        control.check()?;
        let entry = zip.by_index(index).map_err(json_error)?;
        let name = entry.name().map_err(json_error)?;
        if name.contains("vbaProject")
            || name.contains("externalLinks/")
            || name.contains("encryptedPackage")
        {
            return Err(invalid("不支持宏、外部链接或加密工作簿"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| invalid("Excel 展开大小溢出"))?;
        if total > 512 * 1024 * 1024 || entry.size() > entry.compressed_size().saturating_mul(100) {
            return Err(invalid("Excel 展开大小或压缩比超过限制"));
        }
    }
    Xlsx::new(Cursor::new(bytes)).map_err(json_error)
}
fn table(book: &mut Xlsx<Cursor<&[u8]>>, name: &str) -> Result<Vec<Vec<String>>, AppError> {
    let formulas = book.worksheet_formula(name).map_err(json_error)?;
    if formulas.used_cells().any(|(_, _, value)| !value.is_empty()) {
        return Err(invalid(format!("工作表「{name}」含公式，请先转换为值")));
    }
    let range = book.worksheet_range(name).map_err(json_error)?;
    if range
        .start()
        .is_some_and(|(row, column)| row != 0 || column != 0)
    {
        return Err(invalid(format!("工作表「{name}」须从 A1 开始")));
    }
    if range.height() > MAX_WORDS + 1 || range.width() > LABELS.len() {
        return Err(invalid("工作表行数或列数超过限制"));
    }
    range
        .rows()
        .map(|row| row.iter().map(cell).collect())
        .collect()
}
fn selected(book: &Xlsx<Cursor<&[u8]>>, options: &ExcelOptions) -> Result<String, AppError> {
    if !options.columns.is_empty() || !options.pos_mapping.is_empty() {
        return Err(invalid("单表 Excel 不支持列映射或词性映射"));
    }
    if book.sheet_names().len() != 1 {
        return Err(invalid("Excel 只能包含一个工作表"));
    }
    let name = book.sheet_names()[0].clone();
    if options.sheet.as_ref().is_some_and(|chosen| chosen != &name) {
        return Err(invalid("选定的工作表不存在"));
    }
    Ok(name)
}

/// @description 检查单表 Excel 的列和样本。 @param bytes 文件内容；options 列选项；control 取消令牌。 @throws 工作簿结构不符合单表规则时拒绝。
pub(super) fn inspect(
    bytes: &[u8],
    options: &ExcelOptions,
    control: &Control,
) -> Result<serde_json::Value, AppError> {
    let mut book = open(bytes, control)?;
    let sheet = selected(&book, options)?;
    let rows = table(&mut book, &sheet)?;
    let columns = rows.first().cloned().unwrap_or_default();
    let samples: Vec<_> = rows
        .into_iter()
        .skip(usize::from(options.headers))
        .take(20)
        .collect();
    Ok(
        serde_json::json!({"sheets":[sheet.clone()],"sheet":sheet,"columns":columns,"samples":samples}),
    )
}

type Parsed = (
    String,
    Vec<(usize, Result<ExchangeWord, AppError>)>,
    HashSet<String>,
);

fn numbered(value: &str, field: &str) -> Result<Vec<String>, AppError> {
    if !value.contains('\n') {
        if value.starts_with("1. ") {
            return Err(invalid(format!("{field}只有一个值时无需编号")));
        }
        return Ok(vec![value.into()]);
    }
    if !value.starts_with("1. ") {
        return Err(invalid(format!("{field}多项须从 1. 开始逐行编号")));
    }
    value
        .lines()
        .enumerate()
        .map(|(index, line)| {
            line.strip_prefix(&format!("{}. ", index + 1))
                .filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("{field}编号须从 1 连续排列，且内容不能为空")))
        })
        .collect()
}

fn numbered_text(values: &[String]) -> String {
    if values.len() == 1 {
        return values[0].clone();
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| format!("{}. {value}", index + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn examples(jp: &str, cn: &str, senses: usize) -> Result<Vec<ExchangeExample>, AppError> {
    if jp.is_empty() {
        if !cn.is_empty() {
            return Err(invalid("例句中文翻译需要日文例句"));
        }
        return Ok(vec![]);
    }
    let japanese = numbered(jp, "日文例句")?;
    let chinese = if cn.is_empty() {
        vec![String::new(); japanese.len()]
    } else if japanese.len() == 1 {
        vec![cn.into()]
    } else {
        cn.lines()
            .enumerate()
            .map(|(index, line)| {
                let prefix = format!("{}.", index + 1);
                line.strip_prefix(&prefix)
                    .and_then(|text| {
                        text.strip_prefix(' ')
                            .or_else(|| text.is_empty().then_some(text))
                    })
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("例句中文翻译编号必须与日文例句逐条对应"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if japanese.len() != chinese.len() {
        return Err(invalid("例句中文翻译编号必须与日文例句逐条对应"));
    }
    japanese
        .into_iter()
        .zip(chinese)
        .map(|(jp, cn)| {
            let (jp, sense_index) = if let Some(text) = jp.strip_prefix("[无关联] ") {
                (text, None)
            } else if let Some(indexed) = jp.strip_prefix("[义项") {
                let (index, text) = indexed
                    .split_once("] ")
                    .ok_or_else(|| invalid("日文例句义项标记格式错误"))?;
                let number = index
                    .parse::<usize>()
                    .map_err(|_| invalid("日文例句义项编号错误"))?;
                if number == 0 || number > senses {
                    return Err(invalid("日文例句义项编号超出范围"));
                }
                (text, Some(number - 1))
            } else {
                (jp.as_str(), Some(0))
            };
            Ok(ExchangeExample {
                jp: jp.into(),
                cn: (!cn.is_empty()).then_some(cn),
                sense_index,
            })
        })
        .collect()
}

fn export_text(values: &[String], field: &str) -> Result<String, AppError> {
    if values.iter().any(|value| value.contains('\n'))
        || (values.len() == 1 && values[0].starts_with("1. "))
    {
        return Err(invalid(format!(
            "{field}与编号行格式冲突，请修改内容后再导出 Excel"
        )));
    }
    Ok(numbered_text(values))
}

fn export_examples(examples: &[ExchangeExample]) -> Result<(String, String), AppError> {
    let japanese = examples
        .iter()
        .map(|example| {
            if example.jp.contains('\n')
                || example.jp.starts_with("[义项")
                || example.jp.starts_with("[无关联] ")
            {
                return Err(invalid(
                    "日文例句与编号或义项标记冲突，请修改后再导出 Excel",
                ));
            }
            Ok(match example.sense_index {
                Some(0) => example.jp.clone(),
                Some(index) => format!("[义项{}] {}", index + 1, example.jp),
                None => format!("[无关联] {}", example.jp),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let chinese = examples
        .iter()
        .map(|example| example.cn.as_deref().unwrap_or(""))
        .collect::<Vec<_>>();
    let cn = if examples.len() == 1 {
        chinese[0].into()
    } else if chinese.iter().all(|text| text.is_empty()) {
        String::new()
    } else {
        chinese
            .iter()
            .enumerate()
            .map(|(index, text)| {
                format!(
                    "{}.{}{}",
                    index + 1,
                    if text.is_empty() { "" } else { " " },
                    text
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok((export_text(&japanese, "日文例句")?, cn))
}

/// @description 将固定十列的单表 Excel 转为逐行词条；音调拍数由假名推导。 @param bytes 文件内容；options 列设置；namespace 稳定导入来源；control 任务控制。 @throws 缺少必填列、格式错误或非法单元格时报错。
pub(super) fn parse(
    bytes: &[u8],
    options: &ExcelOptions,
    namespace: &str,
    control: &Control,
) -> Result<Parsed, AppError> {
    let mut book = open(bytes, control)?;
    let sheet = selected(&book, options)?;
    let rows = table(&mut book, &sheet)?;
    if !options.headers
        || rows
            .first()
            .is_none_or(|row| row.iter().map(String::as_str).collect::<Vec<_>>() != LABELS)
    {
        return Err(invalid("Excel 表头必须与模板的十列一致"));
    }
    let formulas = book.worksheet_formula(&sheet).map_err(json_error)?;
    let raw = book.worksheet_range(&sheet).map_err(json_error)?;
    let mut result = Vec::new();
    for (index, row) in rows.into_iter().enumerate().skip(1) {
        control.check()?;
        if row.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        let word = (|| {
            if formulas
                .used_cells()
                .any(|(r, _, formula)| r == index && !formula.is_empty())
            {
                return Err(invalid("含公式，请先转换为值"));
            }
            if raw
                .get_value((index as u32, 0))
                .is_some_and(|value| !matches!(value, Data::Empty | Data::String(_)))
            {
                return Err(invalid("词条标识必须为文本"));
            }
            let field = |column: usize| row.get(column).map(String::as_str).unwrap_or("").trim();
            if [1, 2, 3].iter().any(|column| field(*column).is_empty()) {
                return Err(invalid("写法、假名、释义不得为空"));
            }
            let senses = numbered(field(3), "释义")?
                .into_iter()
                .map(|meaning| ExchangeSense { meaning })
                .collect::<Vec<_>>();
            let pitch_accents = if field(5).is_empty() {
                vec![]
            } else {
                field(5)
                    .split('/')
                    .map(|value| {
                        Ok(ExchangePitch {
                            downstep: value
                                .trim()
                                .parse()
                                .map_err(|_| invalid("音调必须为斜杠分隔的非负整数"))?,
                        })
                    })
                    .collect::<Result<Vec<_>, AppError>>()?
            };
            let examples = examples(field(7), field(8), senses.len())?;
            let mut word = ExchangeWord {
                external_id: (!field(0).is_empty()).then(|| field(0).into()),
                kanji: field(1).into(),
                kana: field(2).into(),
                senses,
                parts_of_speech: field(4)
                    .split('|')
                    .filter(|value| !value.is_empty())
                    .map(|value| {
                        options
                            .pos_mapping
                            .get(value)
                            .cloned()
                            .unwrap_or_else(|| value.into())
                    })
                    .collect(),
                pitch_accents,
                jlpt: (!field(6).is_empty())
                    .then(|| format!("N{}", field(6).trim_start_matches(['N', 'n']))),
                examples,
                note: field(9).into(),
            };
            word.validate()?;
            Ok(word)
        })()
        .map_err(|error: AppError| {
            invalid(format!(
                "工作表「{sheet}」第 {} 行：{}",
                index + 1,
                error.message
            ))
        });
        result.push((index + 1, word));
        control
            .processed
            .store(result.len(), std::sync::atomic::Ordering::Release);
    }
    super::super::validation::text(namespace, 1, 1000)?;
    Ok((
        namespace.into(),
        result,
        [
            "external_id",
            "kanji",
            "kana",
            "senses",
            "parts_of_speech",
            "pitch_accents",
            "jlpt",
            "examples",
            "note",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    ))
}

/// @description 写出十列单表并复读校验，不截断多义项等无法表达的内容。 @param path 输出路径；namespace 来源；words 词条；control 任务状态。 @throws 复杂词条或格式限制不允许单表无损表示时拒绝。
pub(super) fn write(
    path: &Path,
    namespace: &str,
    words: &[ExchangeWord],
    control: &Control,
) -> Result<(), AppError> {
    super::super::validation::text(namespace, 1, 1000)?;
    let mut book = Workbook::new();
    let chinese = Format::new().set_font_name("Microsoft YaHei");
    let japanese = Format::new().set_font_name("Yu Gothic");
    let sheet = book.add_worksheet().set_name("词条").map_err(json_error)?;
    sheet.set_freeze_panes(1, 0).map_err(json_error)?;
    for (column, name) in LABELS.iter().enumerate() {
        sheet
            .set_column_width(column as u16, if column == 0 { 38 } else { 20 })
            .map_err(json_error)?;
        sheet
            .set_column_format(
                column as u16,
                if [1, 2, 7].contains(&column) {
                    &japanese
                } else {
                    &chinese
                },
            )
            .map_err(json_error)?;
        sheet
            .write_string_with_format(0, column as u16, *name, &chinese)
            .map_err(json_error)?;
    }
    for (index, word) in words.iter().enumerate() {
        control.check()?;
        if word.kanji.trim().is_empty() {
            return Err(invalid("写法不得为空"));
        }
        let (jp, cn) = export_examples(&word.examples)?;
        let row = [
            word.external_id.clone().unwrap_or_default(),
            word.kanji.clone(),
            word.kana.clone(),
            export_text(
                &word
                    .senses
                    .iter()
                    .map(|s| s.meaning.clone())
                    .collect::<Vec<_>>(),
                "释义",
            )?,
            word.parts_of_speech.join("|"),
            word.pitch_accents
                .iter()
                .map(|p| p.downstep.to_string())
                .collect::<Vec<_>>()
                .join("/"),
            word.jlpt.clone().unwrap_or_default(),
            jp,
            cn,
            word.note.clone(),
        ];
        if row
            .iter()
            .any(|value| value.encode_utf16().count() > 32_767)
            || index >= 1_048_575
        {
            return Err(invalid("Excel 单元格或行数超限，请使用 JSON"));
        }
        for (column, value) in row.iter().enumerate() {
            sheet
                .write_string_with_format(
                    (index + 1) as u32,
                    column as u16,
                    value,
                    if [1, 2, 7].contains(&column) {
                        &japanese
                    } else {
                        &chinese
                    },
                )
                .map_err(json_error)?;
        }
        control
            .processed
            .store(index + 1, std::sync::atomic::Ordering::Release);
    }
    book.save(path).map_err(json_error)?;
    let bytes = super::read_source(path)?;
    let (_, parsed, _) = parse(&bytes, &ExcelOptions::default(), namespace, control)?;
    if parsed.len() != words.len() || parsed.iter().any(|(_, word)| word.is_err()) {
        return Err(invalid("Excel 输出复读校验失败"));
    }
    Ok(())
}
