use serde::Serialize;

/// @description 向界面提供稳定的错误码、消息与重试语义。
/// @param 不适用，数据类型。
/// @throws 不适用。
#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    /// @description 构造稳定的业务错误并标记可重试的数据库占用。
    /// @param code 业务错误码；message 用户可读消息。
    /// @throws 无。
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code == "DB_BUSY",
        }
    }
    /// @description 将输入或业务约束失败转换为统一错误。
    /// @param message 用户可读的失败原因。
    /// @throws 无。
    pub fn validation(message: &str) -> Self {
        Self::new("VALIDATION_FAILED", message)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => {
                Self::new("DB_BUSY", "数据库正在使用，请稍后重试")
            }
            Some(ErrorCode::DiskFull) => Self::new("DISK_FULL", "磁盘空间不足，请释放空间后重试"),
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => {
                Self::new("DB_CORRUPT", "数据库损坏或不是有效数据库；原文件已保留")
            }
            Some(ErrorCode::ConstraintViolation) => {
                Self::new("VALIDATION_FAILED", "数据不符合完整性约束")
            }
            _ => Self::new(
                "DATABASE_ERROR",
                "数据库操作失败；请保留数据并检查访问权限或备份",
            ),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self::new("IO_ERROR", "无法访问本地数据，请检查目录权限与可用空间")
    }
}
