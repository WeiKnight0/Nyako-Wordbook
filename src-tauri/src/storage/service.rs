use super::{migrations, settings, StorageStatus};
use crate::error::AppError;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

/// @description 由专用线程串行执行正式数据库请求。
/// @param 不适用，服务类型。
/// @throws 创建及请求方法通过 Result 返回错误。
pub struct StorageService {
    sender: mpsc::Sender<Request>,
    thread: Option<thread::JoinHandle<()>>,
}

enum Request {
    Status(mpsc::Sender<Result<StorageStatus, AppError>>),
    SaveSetting {
        key: String,
        value: String,
        revision: Option<i64>,
        reply: mpsc::Sender<Result<i64, AppError>>,
    },
    Shutdown,
}

fn status(conn: &Connection, root: &Path) -> Result<StorageStatus, AppError> {
    Ok(StorageStatus {
        state: "ready",
        schema_version: Some(migrations::MIGRATIONS.len()),
        data_directory: root.display().to_string(),
        library_count: Some(conn.query_row("SELECT count(*) FROM libraries", [], |r| r.get(0))?),
        word_count: Some(conn.query_row("SELECT count(*) FROM words", [], |r| r.get(0))?),
        error: None,
    })
}

fn open(root: &Path) -> Result<Connection, AppError> {
    for folder in ["data", "backups", "media", "staging"] {
        std::fs::create_dir_all(root.join(folder))?;
    }
    migrations::open_database(&root.join("data/main.sqlite3"), &root.join("backups"))
}

fn worker(
    root: PathBuf,
    receiver: mpsc::Receiver<Request>,
    initialized: mpsc::SyncSender<Result<(), AppError>>,
) {
    let mut conn = match open(&root) {
        Ok(conn) => conn,
        Err(error) => {
            let _ = initialized.send(Err(error));
            return;
        }
    };
    if initialized.send(Ok(())).is_err() {
        return;
    }
    for request in receiver {
        match request {
            Request::Status(reply) => {
                let _ = reply.send(status(&conn, &root));
            }
            Request::SaveSetting {
                key,
                value,
                revision,
                reply,
            } => {
                let _ = reply.send(settings::save_setting(&mut conn, &key, &value, revision));
            }
            Request::Shutdown => break,
        }
    }
}

impl StorageService {
    /// @description 初始化数据库后开放串行请求入口。
    /// @param root 独立的应用数据根目录。
    /// @throws 返回目录、数据库或线程初始化错误。
    pub fn start(root: PathBuf) -> Result<Self, AppError> {
        let (sender, receiver) = mpsc::channel();
        let (initialized, ready) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("nyako-storage".into())
            .spawn(move || worker(root, receiver, initialized))?;
        match ready.recv() {
            Ok(Ok(())) => Ok(Self {
                sender,
                thread: Some(handle),
            }),
            result => {
                let _ = handle.join();
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => AppError::new("STORAGE_UNAVAILABLE", "存储初始化任务已停止"),
                })
            }
        }
    }

    fn request<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, AppError>>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = mpsc::channel();
        self.sender
            .send(build(reply))
            .map_err(|_| AppError::new("STORAGE_UNAVAILABLE", "存储服务已停止"))?;
        response
            .recv()
            .map_err(|_| AppError::new("STORAGE_UNAVAILABLE", "存储任务未返回结果"))?
    }

    /// @description 读取正式库的可用状态及计数。
    /// @param 无。
    /// @throws 返回服务停止或数据库查询错误。
    pub fn status(&self) -> Result<StorageStatus, AppError> {
        self.request(Request::Status)
    }

    /// @description 在存储线程中保存经过版本检查的设置。
    /// @param key 设置名称；value JSON 内容；revision 读取时的版本。
    /// @throws 返回服务、校验、版本冲突或数据库错误。
    pub fn save_setting(
        &self,
        key: String,
        value: String,
        revision: Option<i64>,
    ) -> Result<i64, AppError> {
        self.request(|reply| Request::SaveSetting {
            key,
            value,
            revision,
            reply,
        })
    }
}

impl Drop for StorageService {
    fn drop(&mut self) {
        let _ = self.sender.send(Request::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
