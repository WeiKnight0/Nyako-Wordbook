use super::{excel, format::*, jobs::Control, output};
use crate::error::AppError;
use std::{fs::OpenOptions, path::Path};

/// @description 生成可直接填写并重新导入的 Excel 示例工作簿。
/// @param path 用户输出路径；expected 覆盖确认；control 任务状态。
/// @throws 写入失败或目标变化时保留原文件。
pub(super) fn save(
    path: &Path,
    expected: Option<&output::Fingerprint>,
    control: &Control,
) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::validation("请选择完整输出路径"))?;
    let temporary = parent.join(format!(".nyako-template-{}.tmp", id()));
    let result = (|| {
        let word = ExchangeWord {
            external_id: Some("sample-cat-001".into()),
            kanji: "猫".into(),
            kana: "ねこ".into(),
            parts_of_speech: vec!["noun".into()],
            jlpt: Some("N5".into()),
            pitch_accents: vec![ExchangePitch { downstep: 1 }],
            senses: vec![ExchangeSense {
                meaning: "猫".into(),
            }],
            examples: vec![ExchangeExample {
                jp: "猫がいます。".into(),
                cn: Some("有一只猫。".into()),
                sense_index: Some(0),
            }],
            note: "这是示例词条，请按需修改或删除；新增词条使用新的唯一标识。".into(),
        };
        excel::write(
            &temporary,
            &format!("nyako:template:{}", id()),
            &[word],
            control,
        )?;
        OpenOptions::new()
            .write(true)
            .open(&temporary)?
            .sync_all()?;
        control.publish(|| output::publish(&temporary, path, expected))
    })();
    if temporary.exists() {
        std::fs::remove_file(temporary)?;
    }
    result
}
