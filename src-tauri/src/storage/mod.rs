#[cfg(feature = "benchmark")]
pub mod benchmark;
pub(crate) mod exchange;
mod libraries;
mod migrations;
pub mod models;
mod query;
mod service;
mod settings;
mod validation;
mod words;

use crate::error::AppError;
use serde::Serialize;
pub use service::{StorageService, VocabularyRequest};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

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
    fn initializing(root: &std::path::Path) -> Self {
        Self {
            state: "initializing",
            schema_version: None,
            data_directory: root.display().to_string(),
            library_count: None,
            word_count: None,
            error: None,
        }
    }

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
    Initializing,
    Ready(StorageService),
    Blocked(AppError),
}

/// @description 在后台完成正式库检查并向窗口报告可用状态。
/// @param 不适用，应用状态类型。
/// @throws 初始化错误由状态报告，不中止窗口。
pub(crate) struct StorageState {
    root: PathBuf,
    availability: Arc<Mutex<Availability>>,
}

impl StorageState {
    /// @description 在独立线程打开正式库，使启动界面无需等待校验。
    /// @param root 新版应用数据根目录。
    /// @throws 线程启动失败时直接报告受阻。
    pub(crate) fn start(root: PathBuf) -> Self {
        let availability = Arc::new(Mutex::new(Availability::Initializing));
        let state = Self {
            root: root.clone(),
            availability: availability.clone(),
        };
        if let Err(error) = thread::Builder::new()
            .name("nyako-initialize".into())
            .spawn(move || {
                let result = StorageService::start(root);
                if let Ok(mut current) = availability.lock() {
                    *current = match result {
                        Ok(service) => Availability::Ready(service),
                        Err(error) => Availability::Blocked(error),
                    };
                }
            })
        {
            if let Ok(mut current) = state.availability.lock() {
                *current =
                    Availability::Blocked(AppError::new("STORAGE_UNAVAILABLE", error.to_string()));
            }
        }
        state
    }

    /// @description 同步初始化供隔离测试和命令行验证使用。
    /// @param root 独立数据根目录。
    /// @throws 初始化错误转为受阻状态。
    #[cfg(test)]
    pub(crate) fn initialize(root: PathBuf) -> Self {
        let availability = match StorageService::start(root.clone()) {
            Ok(service) => Availability::Ready(service),
            Err(error) => Availability::Blocked(error),
        };
        Self {
            root,
            availability: Arc::new(Mutex::new(availability)),
        }
    }

    fn with_service<T>(
        &self,
        action: impl FnOnce(&StorageService) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        match &*self
            .availability
            .lock()
            .map_err(|_| AppError::new("STORAGE_UNAVAILABLE", "存储状态不可用"))?
        {
            Availability::Initializing => Err(AppError::new(
                "STORAGE_INITIALIZING",
                "正在检查词库，请稍候",
            )),
            Availability::Ready(service) => action(service),
            Availability::Blocked(error) => Err(error.clone()),
        }
    }

    /// @description 仅在正式库就绪时执行词汇请求。
    /// @param request 已反序列化的业务操作。
    /// @throws 返回初始化中、受阻原因或业务错误。
    pub(crate) fn vocabulary(
        &self,
        request: VocabularyRequest,
    ) -> Result<serde_json::Value, AppError> {
        self.with_service(|service| service.vocabulary(request))
    }

    /// @description 仅在正式库就绪时执行数据交换请求。
    /// @param request 文件预览或确认后的数据操作；control 任务令牌。
    /// @throws 返回初始化中、受阻、格式或事务错误。
    pub(crate) fn exchange(
        &self,
        request: exchange::ExchangeRequest,
        control: std::sync::Arc<exchange::jobs::Control>,
    ) -> Result<serde_json::Value, AppError> {
        self.with_service(|service| {
            let request = match request {
                exchange::ExchangeRequest::Preview { path, options } => {
                    let preview =
                        exchange::preview_controlled(&self.root, &path, &options, &control)?;
                    exchange::ExchangeRequest::PreparedPreview { preview }
                }
                exchange::ExchangeRequest::InspectExcel { path, options } => {
                    return exchange::inspect_excel(&path, &options, &control);
                }
                request => request,
            };
            service.exchange(request, control)
        })
    }

    /// @description 将初始化、业务就绪或失败原因呈现为存储状态。
    /// @param 无。
    /// @throws 不抛出；状态锁错误转换为受阻状态。
    pub(crate) fn status(&self) -> StorageStatus {
        match self.availability.lock() {
            Ok(current) => match &*current {
                Availability::Initializing => StorageStatus::initializing(&self.root),
                Availability::Ready(service) => service
                    .status()
                    .unwrap_or_else(|error| StorageStatus::blocked(&self.root, error)),
                Availability::Blocked(error) => StorageStatus::blocked(&self.root, error.clone()),
            },
            Err(_) => StorageStatus::blocked(
                &self.root,
                AppError::new("STORAGE_UNAVAILABLE", "存储状态不可用"),
            ),
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
