#[cfg(feature = "benchmark")]
pub mod benchmark;
pub(crate) mod exchange;
mod libraries;
mod migrations;
pub mod models;
mod query;
mod service;
mod settings;
mod tags;
mod validation;
mod words;

use crate::error::AppError;
use serde::Serialize;
pub use service::{StorageService, VocabularyRequest};
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

    /// @description 仅在正式库就绪时执行词汇请求。
    /// @param request 已反序列化的业务操作。
    /// @throws 返回原始受阻原因或业务执行错误。
    pub(crate) fn vocabulary(
        &self,
        request: VocabularyRequest,
    ) -> Result<serde_json::Value, AppError> {
        match &self.availability {
            Availability::Ready(service) => service.vocabulary(request),
            Availability::Blocked(error) => Err(error.clone()),
        }
    }

    /// @description 仅在正式库就绪时执行数据交换请求。
    /// @param request 文件预览或确认后的数据操作。
    /// @throws 返回存储受阻、格式或事务错误。
    pub(crate) fn exchange(
        &self,
        request: exchange::ExchangeRequest,
        control: std::sync::Arc<exchange::jobs::Control>,
    ) -> Result<serde_json::Value, AppError> {
        match &self.availability {
            Availability::Ready(service) => {
                let request = match request {
                    exchange::ExchangeRequest::Preview { path, options } => {
                        let preview =
                            exchange::preview_controlled(&self.root, &path, &options, &control)?;
                        exchange::ExchangeRequest::PreparedPreview { preview }
                    }
                    exchange::ExchangeRequest::InspectCsv { path, options } => {
                        return exchange::inspect_csv(&path, &options, &control);
                    }
                    request => request,
                };
                service.exchange(request, control)
            }
            Availability::Blocked(error) => Err(error.clone()),
        }
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
#[cfg(test)]
mod vocabulary_tests;
