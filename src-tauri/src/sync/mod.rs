//! Keeps the local outbox and the Michiyo Cloud store in step. Offline mode, a missing cloud address
//! or an unpaired device switch it off entirely; nothing is sent in those cases.
use crate::db::sync::{self as local, Ref};
use crate::{ai, commands, config, secrets};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

static RUNNING: AtomicBool = AtomicBool::new(false);
static WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();
const TICK: Duration = Duration::from_secs(15);
const FULL_SYNC_EVERY: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(15 * 60);

struct Target {
    url: String,
    token: String,
    kinds: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct Status {
    enabled: bool,
    pending: i64,
    last_sync_at: Option<i64>,
    last_error: Option<String>,
    syncing: bool,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn target(app: &tauri::AppHandle) -> Option<Target> {
    let c = config::read(app).ok()?;
    let url = config::string(&c, "cloudUrl", "").trim().trim_end_matches('/').to_string();
    if ai::mode(&c) == "offline" || url.is_empty() || ai::validate_base(&url).is_err() {
        return None;
    }
    Some(Target { url, token: secrets::get("cloud").ok()?, kinds: local::kinds_for(&c["syncScopes"]) })
}

fn event_for(kind: &str) -> &'static str {
    match kind {
        "note" => "notes-changed",
        "habit" | "habit_log" => "habits-changed",
        "event" => "events-changed",
        "reminder" => "reminders-changed",
        _ => "activity-changed",
    }
}

fn error_text(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 => "Thiết bị chưa được ghép nối hoặc đã bị thu hồi. Hãy ghép nối lại.".into(),
        429 => "Michiyo Cloud đang giới hạn lượt gọi. Sẽ thử lại sau.".into(),
        code => format!("Michiyo Cloud trả về HTTP {code}."),
    }
}

fn unreachable() -> String {
    "Không kết nối được Michiyo Cloud. Sẽ thử lại sau.".into()
}

fn refs(value: &Value) -> Vec<Ref> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|r| Some(Ref { kind: r["type"].as_str()?.into(), id: r["id"].as_str()?.into(), rev: r["rev"].as_i64()? }))
                .collect()
        })
        .unwrap_or_default()
}

fn with_db<T>(app: &tauri::AppHandle, work: impl FnOnce(&mut rusqlite::Connection) -> crate::db::DbResult<T>) -> Result<T, String> {
    let state = app.state::<commands::Runtime>();
    let _guard = state.history_lock.lock().map_err(|_| "Không đọc được dữ liệu".to_string())?;
    let mut db = commands::local_db(app)?;
    work(&mut db).map_err(|_| "Không đồng bộ được dữ liệu cục bộ".to_string())
}

async fn send(request: reqwest::RequestBuilder, target: &Target) -> Result<Value, String> {
    let response = request.bearer_auth(&target.token).send().await.map_err(|_| unreachable())?;
    if !response.status().is_success() {
        return Err(error_text(response.status()));
    }
    response.json().await.map_err(|_| "Phản hồi đồng bộ không hợp lệ".to_string())
}

async fn exchange(app: &tauri::AppHandle, target: &Target) -> Result<(), String> {
    let client = app.state::<commands::Runtime>().client.clone();
    // Push until the queue is empty. Rejected notes become conflict copies that are pushed by the next batch.
    for _ in 0..20 {
        let items = with_db(app, |db| local::collect_push(db, &target.kinds))?;
        if items.is_empty() {
            break;
        }
        let records: Vec<Value> = items.iter().map(|i| json!({"type": i.reference.kind, "record": i.record})).collect();
        let reply = send(client.post(format!("{}/v1/sync/push", target.url)).json(&json!({"records": records})), target).await?;
        let (accepted, rejected) = (refs(&reply["accepted"]), refs(&reply["rejected"]));
        with_db(app, |db| {
            local::mark_pushed(db, &accepted)?;
            local::handle_rejected(db, &rejected)?;
            Ok(())
        })?;
        if accepted.is_empty() && rejected.is_empty() {
            return Err("Phản hồi đồng bộ không hợp lệ".into());
        }
    }
    for _ in 0..40 {
        let cursor = with_db(app, |db| local::cursor(db, &target.kinds))?;
        let reply = send(client.get(format!("{}/v1/sync/pull", target.url)).query(&[("cursor", cursor.as_str())]), target).await?;
        let records: Vec<(String, Value)> = reply["records"]
            .as_array()
            .map(|a| a.iter().filter_map(|r| Some((r["type"].as_str()?.to_string(), r["record"].clone()))).collect())
            .unwrap_or_default();
        let next = reply["cursor"].as_str().filter(|c| c.parse::<u64>().is_ok()).ok_or("Phản hồi đồng bộ không hợp lệ")?.to_string();
        let applied = with_db(app, |db| {
            let report = local::apply_remote(db, &records, &target.kinds)?;
            local::set_state(db, "pull_cursor", &next)?;
            Ok(report)
        })?;
        for kind in &applied.kinds {
            let _ = app.emit(event_for(kind), ());
        }
        if reply["more"].as_bool() != Some(true) {
            break;
        }
    }
    Ok(())
}

/// One full push-then-pull cycle. `Ok(false)` means sync is switched off.
pub async fn run_once(app: &tauri::AppHandle) -> Result<bool, String> {
    let Some(target) = target(app) else { return Ok(false) };
    if RUNNING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("Đang đồng bộ".into());
    }
    let result = exchange(app, &target).await;
    RUNNING.store(false, Ordering::SeqCst);
    let _ = with_db(app, |db| {
        match &result {
            Ok(()) => {
                local::set_state(db, "last_sync_at", &now_ms().to_string())?;
                db.execute("DELETE FROM sync_state WHERE key = 'last_error'", [])?;
            }
            Err(e) => local::set_state(db, "last_error", e)?,
        }
        Ok(())
    });
    let _ = app.emit("sync-changed", ());
    result.map(|()| true)
}

pub fn status(app: &tauri::AppHandle) -> Result<Status, String> {
    let target = target(app);
    with_db(app, |db| {
        Ok(Status {
            enabled: target.is_some(),
            pending: target.as_ref().map_or(0, |t| local::pending_count(db, &t.kinds).unwrap_or(0)),
            last_sync_at: local::state(db, "last_sync_at")?.and_then(|v| v.parse().ok()),
            last_error: local::state(db, "last_error")?,
            syncing: RUNNING.load(Ordering::SeqCst),
        })
    })
}

#[tauri::command]
pub fn sync_status(app: tauri::AppHandle) -> Result<Status, String> {
    status(&app)
}

#[tauri::command]
pub async fn sync_now(app: tauri::AppHandle) -> Result<Status, String> {
    if !run_once(&app).await? {
        return Err("Đồng bộ đang tắt: cần chế độ Online/Hybrid, địa chỉ Michiyo Cloud và thiết bị đã ghép nối.".into());
    }
    status(&app)
}

pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_full = Instant::now() - FULL_SYNC_EVERY;
        let mut failures = 0u32;
        let mut hold_until = Instant::now();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(TICK) => {}
                _ = WAKE.notified() => {}
            }
            if Instant::now() < hold_until {
                continue;
            }
            let queued = status(&app).map(|s| s.enabled && s.pending > 0).unwrap_or(false);
            if !queued && last_full.elapsed() < FULL_SYNC_EVERY {
                continue;
            }
            match run_once(&app).await {
                Ok(_) => {
                    last_full = Instant::now();
                    failures = 0;
                }
                Err(_) => {
                    failures = (failures + 1).min(8);
                    hold_until = Instant::now() + (TICK * 2u32.pow(failures)).min(MAX_BACKOFF);
                }
            }
        }
    });
}
