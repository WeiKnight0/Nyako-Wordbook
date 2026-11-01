use crate::{
    error::AppError,
    storage::{StorageState, VocabularyRequest},
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// @description 使用原生对话框选择数据交换文件。
/// @param app 当前应用；save 是否选择输出路径；format 文件扩展名。
/// @throws 返回对话框任务或路径转换错误。
#[tauri::command]
pub(crate) async fn exchange_file(
    app: tauri::AppHandle,
    save: bool,
    format: String,
) -> Result<Option<String>, AppError> {
    if !["json", "xlsx", "nyakobak", "import"].contains(&format.as_str()) {
        return Err(AppError::validation("未知文件类型"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let extensions = if format == "import" {
            vec!["xlsx", "json"]
        } else {
            vec![format.as_str()]
        };
        let dialog = app
            .dialog()
            .file()
            .add_filter("NyakoWordbook 数据", &extensions);
        let file = if save {
            dialog.blocking_save_file()
        } else {
            dialog.blocking_pick_file()
        };
        file.map(|file| {
            file.into_path()
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(|_| AppError::validation("请选择本地文件"))
        })
        .transpose()
    })
    .await
    .map_err(|_| AppError::new("DIALOG_FAILED", "文件对话框未能完成"))?
}

/// @description 在覆盖确认前取得输出目标的内容身份。
/// @param path 用户选择的本地目标路径。
/// @throws 文件访问或目标不是普通文件时返回错误。
#[tauri::command]
pub(crate) async fn exchange_target(
    path: std::path::PathBuf,
) -> Result<Option<crate::storage::exchange::output::Fingerprint>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::storage::exchange::output::fingerprint(&path)
    })
    .await
    .map_err(|_| AppError::new("IO_ERROR", "目标文件检查未完成"))?
}

/// @description 将文件交换交给原生存储服务，避免在界面线程处理文件。
/// @param app 当前应用；request 预览、分页或已确认的提交。
/// @throws 返回文件、格式、冲突及存储错误。
#[tauri::command]
pub(crate) async fn exchange(
    app: tauri::AppHandle,
    request: crate::storage::exchange::ExchangeRequest,
) -> Result<serde_json::Value, AppError> {
    let (id, control) = app
        .state::<crate::storage::exchange::jobs::Jobs>()
        .start()?;
    let task_id = id.clone();
    tauri::async_runtime::spawn(async move {
        let worker = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            worker.state::<StorageState>().exchange(request, control)
        })
        .await
        .unwrap_or_else(|_| {
            Err(AppError::new(
                "TASK_FAILED",
                "文件任务异常停止，操作结果需重新核对",
            ))
        });
        app.state::<crate::storage::exchange::jobs::Jobs>()
            .finish(&task_id, result)
    });
    Ok(serde_json::json!({"job_id":id}))
}

/// @description 独立于存储队列读取或取消后台任务。
/// @param app 当前应用；id 后台任务标识；cancel 是否请求取消。
/// @throws 任务不存在或注册表不可用时返回错误。
#[tauri::command]
pub(crate) fn exchange_status(
    app: tauri::AppHandle,
    id: String,
    cancel: bool,
) -> Result<crate::storage::exchange::jobs::JobStatus, AppError> {
    app.state::<crate::storage::exchange::jobs::Jobs>()
        .status(&id, cancel)
}

/// @description 将正式词汇操作交由串行数据库服务执行。
/// @param app 当前桌面应用；request 类型化业务输入。
/// @throws 返回存储受阻、输入校验、版本冲突或数据库错误。
#[tauri::command]
pub(crate) async fn vocabulary(
    app: tauri::AppHandle,
    request: VocabularyRequest,
) -> Result<serde_json::Value, AppError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<StorageState>().vocabulary(request))
        .await
        .map_err(|_| AppError::new("STORAGE_UNAVAILABLE", "词汇任务未能完成"))?
}
