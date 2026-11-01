use crate::error::AppError;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Default)]
pub(crate) struct Control {
    pub cancel: AtomicBool,
    pub processed: AtomicUsize,
    pub(super) finalizing: Mutex<bool>,
}

impl Control {
    pub(crate) fn check(&self) -> Result<(), AppError> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(AppError::new(
                "TASK_CANCELLED",
                "任务已取消，未提交的修改已回滚",
            ));
        }
        Ok(())
    }

    pub(crate) fn publish<T>(
        &self,
        action: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut finalizing = self
            .finalizing
            .lock()
            .map_err(|_| AppError::new("TASK_FAILED", "任务提交锁失效"))?;
        self.check()?;
        *finalizing = true;
        drop(finalizing);
        action()
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct JobStatus {
    pub id: String,
    pub state: String,
    pub processed: usize,
    pub cancellable: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<AppError>,
}

struct Job {
    control: Arc<Control>,
    status: JobStatus,
}

/// @description 保存独立于界面和数据库队列的后台任务及取消状态。
/// @param 不适用，运行时任务注册表。
/// @throws 锁失效或任务不存在时返回业务错误。
#[derive(Default)]
pub(crate) struct Jobs(Mutex<HashMap<String, Job>>);

impl Jobs {
    pub(crate) fn start(&self) -> Result<(String, Arc<Control>), AppError> {
        let mut jobs = self
            .0
            .lock()
            .map_err(|_| AppError::new("TASK_FAILED", "任务注册表不可用"))?;
        if jobs.values().any(|job| job.status.state == "running") {
            return Err(AppError::new("TASK_BUSY", "请先完成当前文件任务"));
        }
        if jobs.len() >= 100 {
            jobs.retain(|_, job| job.status.state == "running");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let control = Arc::new(Control::default());
        jobs.insert(
            id.clone(),
            Job {
                control: control.clone(),
                status: JobStatus {
                    id: id.clone(),
                    state: "running".into(),
                    processed: 0,
                    cancellable: true,
                    result: None,
                    error: None,
                },
            },
        );
        Ok((id, control))
    }

    pub(crate) fn finish(
        &self,
        id: &str,
        result: Result<serde_json::Value, AppError>,
    ) -> Result<(), AppError> {
        let mut jobs = self
            .0
            .lock()
            .map_err(|_| AppError::new("TASK_FAILED", "任务注册表不可用"))?;
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| AppError::validation("任务不存在"))?;
        job.status.state = match result {
            Ok(value) => {
                job.status.result = Some(value);
                "completed"
            }
            Err(error) => {
                let cancelled = error.code == "TASK_CANCELLED";
                job.status.error = Some(error);
                if cancelled {
                    "cancelled"
                } else {
                    "failed"
                }
            }
        }
        .into();
        Ok(())
    }

    pub(crate) fn status(&self, id: &str, cancel: bool) -> Result<JobStatus, AppError> {
        let jobs = self
            .0
            .lock()
            .map_err(|_| AppError::new("TASK_FAILED", "任务注册表不可用"))?;
        let job = jobs
            .get(id)
            .ok_or_else(|| AppError::validation("任务不存在"))?;
        let finalizing = job
            .control
            .finalizing
            .lock()
            .map_err(|_| AppError::new("TASK_FAILED", "任务提交锁失效"))?;
        let mut status = job.status.clone();
        status.processed = job.control.processed.load(Ordering::Acquire);
        status.cancellable = status.state == "running" && !*finalizing;
        if cancel && status.cancellable {
            job.control.cancel.store(true, Ordering::Release);
        }
        Ok(status)
    }
}
