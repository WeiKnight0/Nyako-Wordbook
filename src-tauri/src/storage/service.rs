use super::{libraries, migrations, models::*, query, settings, words, StorageStatus};
use crate::error::AppError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

/// @description 可提交到串行存储线程的正式词汇操作。
/// @param 不适用，命令契约。
/// @throws 执行失败返回业务错误。
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum VocabularyRequest {
    Libraries,
    SaveLibrary {
        library: Library,
        expected: Option<i64>,
    },
    DeleteLibrary {
        id: String,
        revision: i64,
    },
    GetWord {
        id: String,
    },
    SaveWord {
        content: WordInput,
        expected: Option<i64>,
    },
    ListWords {
        query: WordQuery,
        cursor: Option<String>,
        limit: u32,
    },
    DeleteImpact {
        selected: Vec<SelectedWord>,
    },
    Batch {
        selected: Vec<SelectedWord>,
        action: String,
        target: Option<String>,
    },
}

fn execute(
    conn: &mut Connection,
    request: VocabularyRequest,
) -> Result<serde_json::Value, AppError> {
    let value = match request {
        VocabularyRequest::Libraries => serde_json::to_value(libraries::list(conn)?),
        VocabularyRequest::SaveLibrary { library, expected } => {
            serde_json::to_value(libraries::save(conn, library, expected)?)
        }
        VocabularyRequest::DeleteLibrary { id, revision } => {
            serde_json::to_value(libraries::delete(conn, &id, revision)?)
        }
        VocabularyRequest::GetWord { id } => serde_json::to_value(words::get(conn, &id)?),
        VocabularyRequest::SaveWord { content, expected } => {
            serde_json::to_value(words::save(conn, content, expected)?)
        }
        VocabularyRequest::ListWords {
            query,
            cursor,
            limit,
        } => serde_json::to_value(query::list(conn, query, cursor, limit)?),
        VocabularyRequest::DeleteImpact { selected } => {
            serde_json::to_value(words::impact(conn, &selected)?)
        }
        VocabularyRequest::Batch {
            selected,
            action,
            target,
        } => serde_json::to_value(words::batch(conn, selected, &action, target)?),
    };
    value.map_err(|_| AppError::validation("业务结果不可序列化"))
}

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
    Vocabulary(
        Box<VocabularyRequest>,
        mpsc::Sender<Result<serde_json::Value, AppError>>,
    ),
    Exchange(
        super::exchange::ExchangeRequest,
        std::sync::Arc<super::exchange::jobs::Control>,
        mpsc::Sender<Result<serde_json::Value, AppError>>,
    ),
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
    super::exchange::restore::recover(root)?;
    for folder in ["data", "backups", "media", "staging"] {
        std::fs::create_dir_all(root.join(folder))?;
    }
    let _timing = crate::timing::Timing::start("storage/open_total");
    let conn = migrations::open_database(&root.join("data/main.sqlite3"), &root.join("backups"))?;
    query::register(&conn)?;
    Ok(conn)
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
                let result = if root.join("restore-journal.json").exists() {
                    Err(AppError::new(
                        "MAINTENANCE",
                        "恢复已安排，请关闭并重新启动应用",
                    ))
                } else {
                    settings::save_setting(&mut conn, &key, &value, revision)
                };
                let _ = reply.send(result);
            }
            Request::Vocabulary(request, reply) => {
                let read_only = matches!(
                    *request,
                    VocabularyRequest::Libraries
                        | VocabularyRequest::GetWord { .. }
                        | VocabularyRequest::ListWords { .. }
                        | VocabularyRequest::DeleteImpact { .. }
                );
                let result = if root.join("restore-journal.json").exists() && !read_only {
                    Err(AppError::new(
                        "MAINTENANCE",
                        "恢复已安排，请关闭并重新启动应用",
                    ))
                } else {
                    execute(&mut conn, *request)
                };
                let _ = reply.send(result);
            }
            Request::Exchange(request, control, reply) => {
                let result = if root.join("restore-journal.json").exists() {
                    Err(AppError::new(
                        "MAINTENANCE",
                        "恢复已安排，请关闭并重新启动应用",
                    ))
                } else {
                    super::exchange::execute(&mut conn, &root, request, &control)
                };
                let _ = reply.send(result);
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

    /// @description 在正式库线程上执行类型化词汇操作。
    /// @param request 包含完整边界输入的业务命令。
    /// @throws 返回校验、冲突、数据库或服务错误。
    pub fn vocabulary(&self, request: VocabularyRequest) -> Result<serde_json::Value, AppError> {
        self.request(|reply| Request::Vocabulary(Box::new(request), reply))
    }

    /// @description 在正式库写队列执行已预览的数据交换请求。
    /// @param request 数据交换输入。
    /// @throws 返回文件、格式、冲突或数据库错误。
    pub(crate) fn exchange(
        &self,
        request: super::exchange::ExchangeRequest,
        control: std::sync::Arc<super::exchange::jobs::Control>,
    ) -> Result<serde_json::Value, AppError> {
        self.request(|reply| Request::Exchange(request, control, reply))
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
