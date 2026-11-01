#[cfg(feature = "acceptance")]
mod acceptance;
mod commands;
pub mod database;
mod error;
mod scheduler;
pub mod storage;
mod timing;
use commands::{exchange, exchange_file, exchange_status, exchange_target, vocabulary};

use error::AppError;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;

struct ProbeState {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

#[derive(Serialize)]
struct Health {
    app_version: &'static str,
    sqlite_version: String,
    scheduler: &'static str,
    platform: &'static str,
    acceptance_measure: bool,
}

#[tauri::command]
fn health() -> Health {
    Health {
        app_version: env!("CARGO_PKG_VERSION"),
        sqlite_version: rusqlite::version().into(),
        scheduler: "FSRS 6 · fsrs-rs 6.6.2 · BSD-3-Clause",
        platform: std::env::consts::OS,
        acceptance_measure: cfg!(feature = "acceptance")
            && std::env::var_os("NYAKO_ACCEPTANCE_MEASURE").is_some(),
    }
}

#[tauri::command]
async fn run_diagnostics(
    state: tauri::State<'_, ProbeState>,
) -> Result<database::Diagnostics, AppError> {
    let path = state.path.clone();
    let lock = state.lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock
            .lock()
            .map_err(|_| AppError::validation("数据库服务暂不可用"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        database::initialize(&path, 2000)?;
        database::diagnostics(&path)
    })
    .await
    .map_err(|_| AppError::validation("验证任务未能完成"))?
}

#[tauri::command]
async fn search_words(
    state: tauri::State<'_, ProbeState>,
    query: String,
    after: i64,
    limit: u32,
) -> Result<database::SearchPage, AppError> {
    let path = state.path.clone();
    let lock = state.lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock
            .lock()
            .map_err(|_| AppError::validation("数据库服务暂不可用"))?;
        database::search(&path, &query, after, limit)
    })
    .await
    .map_err(|_| AppError::validation("查询任务未能完成"))?
}

#[tauri::command]
fn scheduler_preview() -> Result<Vec<scheduler::RatingPreview>, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AppError::validation("系统时间无效"))?
        .as_millis() as i64;
    scheduler::preview(now)
}

#[tauri::command]
async fn storage_status(app: tauri::AppHandle) -> Result<storage::StorageStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<storage::StorageState>().status())
        .await
        .map_err(|_| AppError::new("STORAGE_UNAVAILABLE", "无法读取存储状态"))
}

/// @description 启动桌面窗口并注册原生业务入口。
/// @param 无。
/// @throws 无法启动运行时或验收目录无效时 panic。
pub fn run() {
    let mut context = tauri::generate_context!();
    #[cfg(feature = "acceptance")]
    let acceptance_root = {
        let root = acceptance::data_root().expect("验收目录无效；未访问默认数据目录");
        context.config_mut().identifier = "io.nyako.wordbook.acceptance".into();
        for window in &mut context.config_mut().app.windows {
            window.data_directory = Some(root.join("webview"));
            window.title = "NyakoWordbook · 隔离验收".into();
        }
        root
    };
    #[cfg(not(feature = "acceptance"))]
    let _ = &mut context;
    #[cfg(feature = "acceptance")]
    let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
        health,
        run_diagnostics,
        search_words,
        scheduler_preview,
        storage_status,
        exchange,
        exchange_file,
        exchange_status,
        exchange_target,
        vocabulary,
        acceptance::acceptance_sample
    ];
    #[cfg(not(feature = "acceptance"))]
    let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
        health,
        run_diagnostics,
        search_words,
        scheduler_preview,
        storage_status,
        exchange,
        exchange_file,
        exchange_status,
        exchange_target,
        vocabulary
    ];
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            #[cfg(feature = "acceptance")]
            let root = acceptance_root.clone();
            #[cfg(not(feature = "acceptance"))]
            let root = app.path().app_data_dir()?;
            app.manage(storage::exchange::jobs::Jobs::default());
            app.manage(storage::StorageState::start(root.clone()));
            app.manage(ProbeState {
                path: root.join("diagnostics/probe.sqlite3"),
                lock: Arc::new(Mutex::new(())),
            });
            Ok(())
        })
        .invoke_handler(handler)
        .run(context)
        .expect("无法启动 NyakoWordbook");
}
