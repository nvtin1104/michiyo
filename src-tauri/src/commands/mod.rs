pub mod ai;
pub mod desktop;
pub mod motion;
pub mod skins;
pub mod workspace;

pub use ai::*;
pub use desktop::*;
pub use workspace::*;

use crate::providers::Translation;
use serde_json::Value;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

pub struct Runtime {
    pub history_lock: Mutex<()>,
    pub db: Mutex<Option<rusqlite::Connection>>,
    pub history_imported: std::sync::atomic::AtomicBool,
    pub clipboard_watch: std::sync::atomic::AtomicBool,
    pub client: reqwest::Client,
    pub cache: Mutex<lru::LruCache<String, Translation>>,
    pub jobs: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
    pub mask: Mutex<Vec<bool>>,
    pub shortcuts: Mutex<Vec<(String, &'static str)>>,
    pub capturing: std::sync::atomic::AtomicBool,
    pub pending_selection: Mutex<Option<String>>,
    pub pending_translation_draft: Mutex<Option<Value>>,
    pub pending_shell_chat_draft: Mutex<Option<Value>>,
    pub pending_capture_error: Mutex<Option<String>>,
    pub ignored_clipboard: Mutex<Option<String>>,
    pub pending_chat: Mutex<Option<String>>,
    pub anchor: Mutex<Option<String>>,
    pub move_generation: std::sync::atomic::AtomicU64,
    pub move_save_active: std::sync::atomic::AtomicBool,
    pub surface_scan_active: std::sync::atomic::AtomicBool,
    pub local_pull: Arc<crate::ai::ollama::PullTracker>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            history_lock: Mutex::new(()),
            db: Mutex::new(None),
            history_imported: std::sync::atomic::AtomicBool::new(false),
            clipboard_watch: std::sync::atomic::AtomicBool::new(false),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(60))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap(),
            cache: Mutex::new(lru::LruCache::new(NonZeroUsize::new(128).unwrap())),
            jobs: Mutex::new(HashMap::new()),
            mask: Mutex::new(vec![false; 160 * 160]),
            shortcuts: Mutex::new(Vec::new()),
            capturing: std::sync::atomic::AtomicBool::new(false),
            pending_selection: Mutex::new(None),
            pending_translation_draft: Mutex::new(None),
            pending_shell_chat_draft: Mutex::new(None),
            pending_capture_error: Mutex::new(None),
            ignored_clipboard: Mutex::new(None),
            pending_chat: Mutex::new(None),
            anchor: Mutex::new(None),
            move_generation: std::sync::atomic::AtomicU64::new(0),
            move_save_active: std::sync::atomic::AtomicBool::new(false),
            surface_scan_active: std::sync::atomic::AtomicBool::new(false),
            local_pull: Arc::new(crate::ai::ollama::PullTracker::default()),
        }
    }
}

pub(crate) fn local_db(app: &tauri::AppHandle) -> Result<rusqlite::Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Không tìm thấy thư mục dữ liệu")?;
    std::fs::create_dir_all(&dir).map_err(|_| "Không tạo được thư mục dữ liệu")?;
    crate::db::open(&dir.join("michiyo.db")).map_err(|_| "Không mở được cơ sở dữ liệu".to_string())
}

pub(crate) fn with_local_db<T>(
    app: &tauri::AppHandle,
    state: &Runtime,
    lock_err: &str,
    work: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| lock_err.to_string())?;
    let mut slot = state.db.lock().map_err(|_| lock_err.to_string())?;
    if slot.is_none() {
        *slot = Some(local_db(app)?);
    }
    work(slot.as_mut().unwrap())
}

pub(crate) fn with_history_db<T>(
    app: &tauri::AppHandle,
    state: &Runtime,
    lock_err: &str,
    work: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    use std::sync::atomic::Ordering;
    with_local_db(app, state, lock_err, |db| {
        if !state.history_imported.load(Ordering::SeqCst) {
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|_| "Không tìm thấy thư mục dữ liệu")?;
            crate::db::import_history(db, &dir.join("history.json")).map_err(|_| {
                "Không nhập được lịch sử cũ. Dữ liệu gốc vẫn được giữ; hãy kiểm tra history.json."
                    .to_string()
            })?;
            state.history_imported.store(true, Ordering::SeqCst);
        }
        work(db)
    })
}

pub(crate) fn record_activity(app: &tauri::AppHandle, state: &Runtime, kind: &str, data: Value) {
    let recorded = with_local_db(app, state, "lock", |db| {
        crate::db::activity::record(db, kind, data).map_err(|e| e.to_string())
    });
    if recorded.is_ok() {
        let _ = app.emit("activity-changed", ());
    }
}
