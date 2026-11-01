use std::time::Instant;

/// @description 记录隔离验收的后端阶段耗时，普通构建不写入文件。
/// @param 无，阶段名称由 start 指定。
/// @throws 析构不能传播错误，写入失败明确输出到标准错误。
pub(crate) struct Timing {
    stage: &'static str,
    started: Instant,
}

impl Timing {
    /// @description 开始一个后端验收阶段，离开作用域时记录耗时。
    /// @param stage 稳定的阶段名称。
    /// @throws 无；写入错误在析构时输出到标准错误。
    pub(crate) fn start(stage: &'static str) -> Self {
        Self {
            stage,
            started: Instant::now(),
        }
    }
}

impl Drop for Timing {
    fn drop(&mut self) {
        #[cfg(feature = "acceptance")]
        if std::env::var_os("NYAKO_ACCEPTANCE_TIMING").is_some() {
            let result = crate::acceptance::record_backend(serde_json::json!({
                "stage": self.stage,
                "elapsed_ms": self.started.elapsed().as_secs_f64() * 1000.0,
                "pid": std::process::id(),
            }));
            if let Err(error) = result {
                eprintln!("验收计时写入失败：{error}");
            }
        }
        #[cfg(not(feature = "acceptance"))]
        let _ = (self.stage, self.started);
    }
}
