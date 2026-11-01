use std::{io::Write, path::PathBuf};

/// @description 仅在隔离验收构建中追加界面测量结果。
/// @param sample 包含测量场景、耗时及行数的 JSON。
/// @throws 返回标记校验、序列化或写入错误。
#[tauri::command]
pub(crate) fn acceptance_sample(sample: serde_json::Value) -> Result<(), String> {
    let root = data_root().map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("frontend.jsonl"))
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", sample).map_err(|e| e.to_string())
}

/// @description 将启动阶段计时追加到标记验收目录。
/// @param sample 阶段名称、进程标识及耗时。
/// @throws 返回目录校验、序列化或文件写入错误。
pub(crate) fn record_backend(sample: serde_json::Value) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_root()?.join("backend-stages.jsonl"))?;
    writeln!(file, "{sample}")
}

/// @description 校验显式标记的隔离验收目录，防止回退访问正式数据。
/// @param 无，目录由 NYAKO_ACCEPTANCE_ROOT 指定。
/// @throws 返回目录缺失、路径非法或标记缺失的 IO 错误。
pub(super) fn data_root() -> Result<PathBuf, std::io::Error> {
    let value = std::env::var_os("NYAKO_ACCEPTANCE_ROOT")
        .ok_or_else(|| std::io::Error::other("验收构建必须设置 NYAKO_ACCEPTANCE_ROOT"))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(std::io::Error::other("验收数据目录必须为绝对路径"));
    }
    std::fs::create_dir_all(&path)?;
    let path = path.canonicalize()?;
    if !path.join(".nyako-acceptance").is_file() {
        return Err(std::io::Error::other("验收目录缺少 .nyako-acceptance 标记"));
    }
    Ok(path)
}
