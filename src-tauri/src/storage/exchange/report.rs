use super::{
    format::{id, json_error},
    import::stage,
    jobs::Control,
    output::{self, Fingerprint},
};
use crate::error::AppError;
use std::{fs::OpenOptions, io::Write, path::Path};

pub(super) fn save(
    root: &Path,
    task: &str,
    path: &Path,
    expected: Option<&Fingerprint>,
    control: &Control,
) -> Result<(), AppError> {
    let staged = stage(root, task)?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::validation("输出路径无效"))?;
    let temporary = parent.join(format!(".nyako-errors-{}.tmp", id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut stmt =
            staged.prepare("SELECT line,error FROM rows WHERE error IS NOT NULL ORDER BY line")?;
        let mut rows = stmt.query([])?;
        file.write_all(b"[")?;
        let mut first = true;
        while let Some(row) = rows.next()? {
            control.check()?;
            let line: i64 = row.get(0)?;
            let reason: String = row.get(1)?;
            if !first {
                file.write_all(b",")?;
            }
            first = false;
            serde_json::to_writer(
                &mut file,
                &serde_json::json!({"line":line,"field":"row","reason":reason}),
            )
            .map_err(json_error)?;
            file.write_all(b"\n")?;
        }
        file.write_all(b"]")?;
        file.sync_all()?;
        drop(file);
        control.publish(|| output::publish(&temporary, path, expected))
    })();
    if temporary.exists() {
        std::fs::remove_file(temporary)?;
    }
    result
}
