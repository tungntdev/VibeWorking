//! Trạng thái dùng chung của ứng dụng (độc lập với Tauri để kiểm thử được).

use crate::auth::{self, Sessions};
use crate::db::{self, User};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use tokio::sync::watch;

pub type Emitter = Box<dyn Fn(Value) + Send + Sync>;

pub struct Inner {
    pub db: Mutex<Connection>,
    pub sessions: Mutex<Sessions>,
    pub running: Mutex<HashMap<i64, watch::Sender<bool>>>,
    pub agent_locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    /// CLI đang cài → kênh huỷ
    pub installing: Mutex<HashMap<String, watch::Sender<bool>>>,
    /// Đầu việc đang chạy → các dòng tiến trình (giờ + nội dung)
    pub progress: Mutex<HashMap<i64, Vec<String>>>,
    pub data_dir: PathBuf,
    pub path_env: RwLock<String>,
    pub emit: Emitter,
}

#[derive(Clone)]
pub struct Core(pub Arc<Inner>);

impl Core {
    pub fn new(conn: Connection, data_dir: PathBuf, path_env: String, emit: Emitter) -> Self {
        Core(Arc::new(Inner {
            db: Mutex::new(conn),
            sessions: Mutex::new(Sessions::default()),
            running: Mutex::new(HashMap::new()),
            agent_locks: Mutex::new(HashMap::new()),
            installing: Mutex::new(HashMap::new()),
            progress: Mutex::new(HashMap::new()),
            data_dir,
            path_env: RwLock::new(path_env),
            emit,
        }))
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.0.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn sessions(&self) -> MutexGuard<'_, Sessions> {
        self.0.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_path_env(&self, p: String) {
        if let Ok(mut w) = self.0.path_env.write() {
            *w = p;
        }
    }

    pub fn path_env(&self) -> String {
        self.0.path_env.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn emit(&self, v: Value) {
        (self.0.emit)(v)
    }

    pub fn workdir(&self, agent_id: i64) -> PathBuf {
        self.0.data_dir.join("workspaces").join(format!("nhan-vien-{agent_id}"))
    }

    pub fn agent_lock(&self, agent_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        let mut m = self.0.agent_locks.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(agent_id).or_default().clone()
    }

    pub fn is_running(&self, mission_id: i64) -> bool {
        self.0.running.lock().map(|m| m.contains_key(&mission_id)).unwrap_or(false)
    }

    /// Ghi nhật ký và phát sự kiện cho giao diện.
    pub fn log(&self, kind: &str, actor: &str, msg: &str, mission: Option<i64>, agent: Option<i64>) {
        let a = db::log(&self.db(), kind, actor, msg, mission, agent);
        if let Ok(a) = a {
            self.emit(serde_json::json!({"type": "activity", "item": a}));
        }
    }

    /// Xác thực phiên và kiểm tra quyền tối thiểu.
    pub fn auth(&self, token: &str, min_role: &str) -> Result<User> {
        let uid = self
            .sessions()
            .touch(token)
            .ok_or_else(|| anyhow!("Phiên đăng nhập đã hết hạn, vui lòng đăng nhập lại"))?;
        let user = db::get_user(&self.db(), uid)?.ok_or_else(|| anyhow!("Tài khoản không tồn tại"))?;
        if !user.active {
            return Err(anyhow!("Tài khoản đã bị khoá"));
        }
        if auth::role_rank(&user.role) < auth::role_rank(min_role) {
            return Err(anyhow!("Bạn không có quyền thực hiện thao tác này"));
        }
        Ok(user)
    }
}
