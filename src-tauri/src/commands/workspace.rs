use super::{with_history_db, with_local_db, Runtime};
use tauri::Emitter;

fn note_error(error: Box<dyn std::error::Error + Send + Sync>) -> String {
    let message = error.to_string();
    match message.as_str() {
        "Ghi chú không hợp lệ"
        | "Mã ghi chú không hợp lệ"
        | "Ghi chú đã thay đổi; hãy tải lại"
        | "Không tìm thấy ghi chú" => message,
        _ => "Không lưu được ghi chú. Hãy thử lại.".into(),
    }
}

#[tauri::command]
pub fn notes_list(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    query: String,
) -> Result<Vec<crate::db::notes::Note>, String> {
    with_local_db(&app, &state, "Không đọc được ghi chú", |db| {
        crate::db::notes::list(db, &query).map_err(|_| "Không tìm được ghi chú".to_string())
    })
}

#[tauri::command]
pub fn notes_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    input: crate::db::notes::NoteInput,
) -> Result<crate::db::notes::Note, String> {
    let note = with_local_db(&app, &state, "Không lưu được ghi chú", |db| {
        crate::db::notes::save(db, input).map_err(note_error)
    })?;
    let _ = app.emit("notes-changed", ());
    Ok(note)
}

#[tauri::command]
pub fn notes_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    rev: i64,
) -> Result<(), String> {
    with_local_db(&app, &state, "Không xóa được ghi chú", |db| {
        crate::db::notes::delete(db, &id, rev).map_err(note_error)
    })?;
    let _ = app.emit("notes-changed", ());
    Ok(())
}

fn habit_error(error: Box<dyn std::error::Error + Send + Sync>) -> String {
    let message = error.to_string();
    match message.as_str() {
        "Thói quen không hợp lệ"
        | "Mã thói quen không hợp lệ"
        | "Thói quen đã thay đổi; hãy tải lại"
        | "Không tìm thấy thói quen"
        | "Ngày không hợp lệ" => message,
        _ => "Không lưu được thói quen. Hãy thử lại.".into(),
    }
}

#[tauri::command]
pub fn habits_list(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    today: String,
) -> Result<Vec<crate::db::habits::HabitView>, String> {
    with_local_db(&app, &state, "Không đọc được thói quen", |db| {
        crate::db::habits::list(db, &today).map_err(habit_error)
    })
}

#[tauri::command]
pub fn habits_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    input: crate::db::habits::HabitInput,
) -> Result<crate::db::habits::Habit, String> {
    let habit = with_local_db(&app, &state, "Không lưu được thói quen", |db| {
        crate::db::habits::save(db, input).map_err(habit_error)
    })?;
    let _ = app.emit("habits-changed", ());
    Ok(habit)
}

#[tauri::command]
pub fn habits_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    rev: i64,
) -> Result<(), String> {
    with_local_db(&app, &state, "Không xóa được thói quen", |db| {
        crate::db::habits::delete(db, &id, rev).map_err(habit_error)
    })?;
    let _ = app.emit("habits-changed", ());
    Ok(())
}

#[tauri::command]
pub fn habits_check_in(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    date: String,
    done: bool,
) -> Result<(), String> {
    with_local_db(&app, &state, "Không lưu được thói quen", |db| {
        crate::db::habits::check_in(db, &id, &date, done).map_err(habit_error)
    })?;
    let _ = app.emit("habits-changed", ());
    Ok(())
}

fn event_error(error: Box<dyn std::error::Error + Send + Sync>) -> String {
    let message = error.to_string();
    match message.as_str() {
        "Sự kiện không hợp lệ"
        | "Mã sự kiện không hợp lệ"
        | "Sự kiện đã thay đổi; hãy tải lại"
        | "Không tìm thấy sự kiện"
        | "Khoảng thời gian không hợp lệ" => message,
        _ => "Không lưu được sự kiện. Hãy thử lại.".into(),
    }
}

#[tauri::command]
pub fn events_range(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    from: i64,
    until: i64,
    tz_offset: i64,
) -> Result<Vec<crate::db::events::Occurrence>, String> {
    with_local_db(&app, &state, "Không đọc được lịch", |db| {
        crate::db::events::range(db, from, until, tz_offset).map_err(event_error)
    })
}

#[tauri::command]
pub fn events_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    input: crate::db::events::EventInput,
) -> Result<crate::db::events::Event, String> {
    let event = with_local_db(&app, &state, "Không lưu được sự kiện", |db| {
        crate::db::events::save(db, input).map_err(event_error)
    })?;
    let _ = app.emit("events-changed", ());
    Ok(event)
}

#[tauri::command]
pub fn events_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    rev: i64,
) -> Result<(), String> {
    with_local_db(&app, &state, "Không xóa được sự kiện", |db| {
        crate::db::events::delete(db, &id, rev).map_err(event_error)
    })?;
    let _ = app.emit("events-changed", ());
    Ok(())
}

#[tauri::command]
pub fn events_set_done(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    rev: i64,
    done: bool,
) -> Result<crate::db::events::Event, String> {
    let event = with_local_db(&app, &state, "Không lưu được sự kiện", |db| {
        crate::db::events::set_done(db, &id, rev, done).map_err(event_error)
    })?;
    let _ = app.emit("events-changed", ());
    Ok(event)
}

fn reminder_error(error: Box<dyn std::error::Error + Send + Sync>) -> String {
    let message = error.to_string();
    match message.as_str() {
        "Không tìm thấy nhắc nhở"
        | "Nhắc nhở không còn chờ xử lý"
        | "Thời gian hoãn không hợp lệ" => message,
        _ => "Không cập nhật được nhắc nhở. Hãy thử lại.".into(),
    }
}

/// Remembers the UI's UTC offset so the background scheduler can evaluate recurrences locally.
#[tauri::command]
pub fn set_timezone(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    offset: i64,
) -> Result<(), String> {
    if offset.abs() > 14 * 60 {
        return Err("Múi giờ không hợp lệ".into());
    }
    with_local_db(&app, &state, "Không lưu được múi giờ", |db| {
        db.execute(
            "INSERT INTO sync_state (key, value) VALUES ('tz_offset', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [offset.to_string()],
        )
        .map(|_| ())
        .map_err(|_| "Không lưu được múi giờ".to_string())
    })
}

#[tauri::command]
pub fn reminders_pending(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<Vec<crate::db::reminders::Pending>, String> {
    with_local_db(&app, &state, "Không đọc được nhắc nhở", |db| {
        crate::db::reminders::pending(db).map_err(reminder_error)
    })
}

#[tauri::command]
pub fn reminders_dismiss(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
) -> Result<(), String> {
    with_local_db(
        &app,
        &state,
        "Không cập nhật được nhắc nhở",
        |db| crate::db::reminders::dismiss(db, &id).map_err(reminder_error),
    )?;
    let _ = app.emit("reminders-changed", ());
    Ok(())
}

#[tauri::command]
pub fn reminders_snooze(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    minutes: i64,
) -> Result<(), String> {
    with_local_db(
        &app,
        &state,
        "Không cập nhật được nhắc nhở",
        |db| crate::db::reminders::snooze(db, &id, minutes).map_err(reminder_error),
    )?;
    let _ = app.emit("reminders-changed", ());
    Ok(())
}

#[tauri::command]
pub fn activity_list(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    kind: Option<String>,
    from: Option<i64>,
    until: Option<i64>,
) -> Result<Vec<crate::db::activity::Activity>, String> {
    with_local_db(
        &app,
        &state,
        "Không đọc được lịch sử hoạt động",
        |db| {
            crate::db::activity::list(db, kind.as_deref(), from, until)
                .map_err(|_| "Không đọc được lịch sử hoạt động".into())
        },
    )
}

#[tauri::command]
pub fn activity_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    ids: Vec<String>,
) -> Result<(), String> {
    with_local_db(
        &app,
        &state,
        "Không xóa được lịch sử hoạt động",
        |db| {
            crate::db::activity::delete(db, &ids)
                .map_err(|_| "Không xóa được lịch sử hoạt động".into())
        },
    )?;
    let _ = app.emit("activity-changed", ());
    Ok(())
}

#[tauri::command]
pub fn activity_delete_range(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    from: i64,
    until: i64,
) -> Result<usize, String> {
    let count = with_local_db(
        &app,
        &state,
        "Không xóa được lịch sử hoạt động",
        |db| {
            crate::db::activity::delete_range(db, from, until)
                .map_err(|_| "Không xóa được lịch sử hoạt động".to_string())
        },
    )?;
    let _ = app.emit("activity-changed", ());
    Ok(count)
}

#[tauri::command]
pub fn history_list(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<Vec<crate::db::Conversation>, String> {
    with_history_db(&app, &state, "Không mở được lịch sử", |db| {
        crate::db::list_history(db).map_err(|_| "Không đọc được lịch sử".to_string())
    })
}

#[tauri::command]
pub fn history_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    conversation: crate::db::Conversation,
    limit: usize,
) -> Result<Vec<crate::db::Conversation>, String> {
    let result = with_history_db(&app, &state, "Không lưu được lịch sử", |db| {
        crate::db::save_history(db, &conversation, limit)
            .map_err(|_| "Không lưu được lịch sử".to_string())
    })?;
    let _ = app.emit("history-changed", ());
    Ok(result)
}

#[tauri::command]
pub fn history_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
) -> Result<Vec<crate::db::Conversation>, String> {
    let result = with_history_db(&app, &state, "Không xóa được lịch sử", |db| {
        crate::db::delete_history(db, &id).map_err(|_| "Không xóa được lịch sử".to_string())
    })?;
    let _ = app.emit("history-changed", ());
    Ok(result)
}
