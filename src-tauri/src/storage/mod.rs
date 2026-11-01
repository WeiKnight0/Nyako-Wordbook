mod migrations;
mod service;
mod settings;

use crate::error::AppError;
use serde::Serialize;
pub use service::StorageService;
use std::path::PathBuf;

/// @description 对外报告正式数据底座的可用性与失败原因。
/// @param 不适用，数据类型。
/// @throws 不适用。
#[derive(Clone, Debug, Serialize)]
pub struct StorageStatus {
    pub state: &'static str,
    pub schema_version: Option<usize>,
    pub data_directory: String,
    pub library_count: Option<i64>,
    pub word_count: Option<i64>,
    pub error: Option<AppError>,
}

impl StorageStatus {
    fn blocked(root: &std::path::Path, error: AppError) -> Self {
        Self {
            state: "blocked",
            schema_version: None,
            data_directory: root.display().to_string(),
            library_count: None,
            word_count: None,
            error: Some(error),
        }
    }
}

enum Availability {
    Ready(StorageService),
    Blocked(AppError),
}

/// @description 保证正式库服务与启动失败状态互斥。
/// @param 不适用，应用状态类型。
/// @throws 初始化错误由状态报告，不中止工程验证窗口。
pub(crate) struct StorageState {
    root: PathBuf,
    availability: Availability,
}

impl StorageState {
    /// @description 打开正式库，失败时保留受阻原因供窗口显示。
    /// @param root 应用数据根目录。
    /// @throws 不抛出；数据库错误转为 Blocked 状态。
    pub(crate) fn initialize(root: PathBuf) -> Self {
        let availability = match StorageService::start(root.clone()) {
            Ok(service) => Availability::Ready(service),
            Err(error) => Availability::Blocked(error),
        };
        Self { root, availability }
    }

    /// @description 将启动或请求失败统一呈现为正式存储状态。
    /// @param 无。
    /// @throws 不抛出；服务错误包含在受阻状态中。
    pub(crate) fn status(&self) -> StorageStatus {
        match &self.availability {
            Availability::Ready(service) => service
                .status()
                .unwrap_or_else(|error| StorageStatus::blocked(&self.root, error)),
            Availability::Blocked(error) => StorageStatus::blocked(&self.root, error.clone()),
        }
    }
}

#[cfg(test)]
use rusqlite::Connection;
#[cfg(test)]
use settings::transaction;
#[cfg(test)]
mod tests;
