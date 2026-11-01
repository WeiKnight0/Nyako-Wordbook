use std::path::PathBuf;

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
