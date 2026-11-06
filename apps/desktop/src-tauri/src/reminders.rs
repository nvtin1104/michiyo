//! Background scheduler: announces due reminders with a system notification.
use crate::{commands, db};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

const TICK: std::time::Duration = std::time::Duration::from_secs(30);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn tick(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<commands::Runtime>();
    let fired = commands::with_local_db(app, &state, "lock", |conn| {
        let offset: i64 = conn
            .query_row(
                "SELECT value FROM sync_state WHERE key = 'tz_offset'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        db::reminders::fire_due(conn, now_ms(), offset).map_err(|e| e.to_string())
    })?;
    if fired.is_empty() {
        return Ok(());
    }
    for item in &fired {
        let _ = app
            .notification()
            .builder()
            .title(if item.kind == "todo" {
                "Việc cần làm"
            } else {
                "Sắp diễn ra"
            })
            .body(&item.title)
            .show();
    }
    let _ = app.emit("reminders-changed", ());
    Ok(())
}

pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        // A failed tick (locked database, missing data dir) is retried on the next one.
        let _ = tick(&app);
    });
}
