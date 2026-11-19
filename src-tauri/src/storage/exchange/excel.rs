use super::{format::*, jobs::Control};
use crate::error::AppError;
use calamine::{Data, Reader, Xlsx};
use rust_xlsxwriter::{Format, Workbook};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io::Cursor, path::Path};

const NAMESPACE_NAME: &str = "_nyako_source_namespace";

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

/// @description 配置固定十列的单表 Excel。 @param sheet 可选工作表名；headers 表头标志。 @throws 多表或缺表头在预览时拒绝。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExcelOptions {
    pub sheet: Option<String>,
    #[serde(default = "yes")]
    pub headers: bool,
}
impl Default for ExcelOptions {
    fn default() -> Self {
        Self {
            sheet: None,
            headers: true,
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

fn source_namespace(book: &Xlsx<Cursor<&[u8]>>, fallback: &str) -> Result<String, AppError> {
    let mut names = book
        .defined_names()
        .iter()
        .filter(|(name, _)| name == NAMESPACE_NAME);
    let Some((_, value)) = names.next() else {
        return super::super::validation::text(fallback, 1, 1000);
    };
    if names.next().is_some() {
        return Err(invalid("Excel 来源命名空间重复"));
    }
    // 只读取常量字符串，绝不解析或执行工作簿中的命名公式。
    let literal = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| invalid("Excel 来源命名空间必须为文本常量"))?;
    if literal.replace("\"\"", "").contains('"') {
        return Err(invalid("Excel 来源命名空间文本转义无效"));
    }
    super::super::validation::text(&literal.replace("\"\"", "\""), 1, 1000)
}

/// @description 将固定十列的单表 Excel 转为逐行词条，文本单元格直接保存。 @param bytes 文件内容；options 列设置；namespace 稳定导入来源；control 任务控制。 @throws 缺少必填列、格式错误或非法单元格时报错。
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
            let mut word = ExchangeWord {
                external_id: (!field(0).is_empty()).then(|| field(0).into()),
                kanji: field(1).into(),
                kana: field(2).into(),
                meaning: field(3).into(),
                parts_of_speech: field(4)
                    .split('|')
                    .filter(|value| !value.is_empty())
                    .map(String::from)
                    .collect(),
                pitch: field(5).into(),
                jlpt: (!field(6).is_empty())
                    .then(|| format!("N{}", field(6).trim_start_matches(['N', 'n']))),
                examples_jp: field(7).into(),
                examples_cn: field(8).into(),
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
    Ok((
        source_namespace(&book, namespace)?,
        result,
        [
            "external_id",
            "kanji",
            "kana",
            "meaning",
            "parts_of_speech",
            "pitch",
            "jlpt",
            "examples_jp",
            "examples_cn",
            "note",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    ))
}

/// @description 写出十列单表并复读校验，保留文本内容和换行。 @param path 输出路径；namespace 导出来源，模板留空以使用用户填写的来源；words 词条；control 任务状态。 @throws 复杂词条或格式限制不允许单表无损表示时拒绝。
pub(super) fn write(
    path: &Path,
    namespace: Option<&str>,
    words: &[ExchangeWord],
    control: &Control,
) -> Result<(), AppError> {
    let namespace = namespace
        .map(|value| super::super::validation::text(value, 1, 1000))
        .transpose()?;
    let mut book = Workbook::new();
    // 模板的来源由用户指定；只有导出数据携带可跨文件保留的身份来源。
    if let Some(namespace) = &namespace {
        book.define_name(
            NAMESPACE_NAME,
            &format!("=\"{}\"", namespace.replace('"', "\"\"")),
        )
        .map_err(json_error)?;
    }
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
        let row = [
            word.external_id.clone().unwrap_or_default(),
            word.kanji.clone(),
            word.kana.clone(),
            word.meaning.clone(),
            word.parts_of_speech.join("|"),
            word.pitch.clone(),
            word.jlpt.clone().unwrap_or_default(),
            word.examples_jp.clone(),
            word.examples_cn.clone(),
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
    let expected_namespace = namespace.as_deref().unwrap_or("nyako:template-check");
    let (parsed_namespace, parsed, _) = parse(
        &bytes,
        &ExcelOptions::default(),
        expected_namespace,
        control,
    )?;
    if parsed_namespace != expected_namespace {
        return Err(invalid("Excel 输出来源校验失败"));
    }
    if parsed.len() != words.len() || parsed.iter().any(|(_, word)| word.is_err()) {
        return Err(invalid("Excel 输出复读校验失败"));
    }
    Ok(())
}
