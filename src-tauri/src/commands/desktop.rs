use super::Runtime;
use crate::config;
use serde_json::{json, Value};
use tauri::{Emitter, Manager};

#[tauri::command]
pub fn set_hit_mask(state: tauri::State<'_, Runtime>, mask: Vec<bool>) -> Result<(), String> {
    if mask.len() != 160 * 160 {
        return Err("Sai kích thước hit mask".into());
    }
    *state.mask.lock().unwrap() = mask;
    Ok(())
}

#[tauri::command]
pub async fn open_chat_in_main(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    draft: Value,
) -> Result<(), String> {
    if draft.to_string().len() > 1_100_000 {
        return Err("Hội thoại quá dài".into());
    }
    *state
        .pending_shell_chat_draft
        .lock()
        .map_err(|_| "Không mở được cuộc trò chuyện")? = Some(draft);
    open_main_impl(app.clone(), "chat".into())?;
    let _ = app.emit_to("main", "focus-composer", ());
    Ok(())
}

#[tauri::command]
pub fn take_shell_chat_draft(state: tauri::State<'_, Runtime>) -> Option<Value> {
    state.pending_shell_chat_draft.lock().ok()?.take()
}

#[tauri::command]
pub async fn open_translation_in_main(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    text: String,
    result: Value,
) -> Result<(), String> {
    if text.len() > 100_000 || result.to_string().len() > 200_000 {
        return Err("Bản dịch quá dài".into());
    }
    *state
        .pending_translation_draft
        .lock()
        .map_err(|_| "Không mở được bản dịch")? = Some(json!({"text": text, "result": result}));
    open_main_impl(app, "translate".into())
}

#[tauri::command]
pub fn take_translation_draft(state: tauri::State<'_, Runtime>) -> Option<Value> {
    state.pending_translation_draft.lock().ok()?.take()
}

#[tauri::command]
pub async fn open_main(app: tauri::AppHandle, route: String) -> Result<(), String> {
    open_main_impl(app, route)
}

// WebviewWindowBuilder must run off the Windows event thread.
pub(crate) fn open_main_impl(app: tauri::AppHandle, route: String) -> Result<(), String> {
    if ![
        "home",
        "chat",
        "translate",
        "notes",
        "habits",
        "calendar",
        "history",
        "bot",
        "settings",
    ]
    .contains(&route.as_str())
    {
        return Err("Trang không hợp lệ".into());
    }
    let was_visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let window = if let Some(window) = app.get_webview_window("main") {
        window
            .emit("shell-navigate", &route)
            .map_err(|e| e.to_string())?;
        if !was_visible {
            let _ = window.emit("shell-opened", &route);
        }
        window
    } else {
        tauri::WebviewWindowBuilder::new(
            &app,
            "main",
            tauri::WebviewUrl::App(format!("index.html?view=main#/{route}").into()),
        )
        .title("Michiyo")
        .inner_size(1040., 720.)
        .min_inner_size(640., 480.)
        .center()
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?
    };
    if let Some(mascot) = app.get_webview_window("mascot") {
        let _ = mascot.hide();
        let _ = app.emit_to("mascot", "mascot-visible", false);
    }
    if let Some(chat) = app.get_webview_window("chat") {
        let _ = chat.hide();
    }
    let _ = window.unminimize();
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    let _ = app.emit_to("main", "focus-composer", ());
    notify_chat_visibility(&app);
    Ok(())
}

pub(crate) fn close_main_to_mascot_impl(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    if let Some(chat) = app.get_webview_window("chat") {
        let _ = chat.hide();
    }
    let _ = ensure_mascot_on_screen(app);
    if let Some(mascot) = app.get_webview_window("mascot") {
        mascot.show().map_err(|e| e.to_string())?;
        let _ = app.emit_to("mascot", "mascot-visible", true);
        let _ = app.emit_to("mascot", "mascot-boom", ());
    }
    notify_chat_visibility(app);
    Ok(())
}

#[tauri::command]
pub fn window_minimize(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.minimize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_close_to_mascot(app: tauri::AppHandle) -> Result<(), String> {
    close_main_to_mascot_impl(&app)
}

pub(crate) fn notify_chat_visibility(app: &tauri::AppHandle) {
    let visible = ["chat", "main"].iter().any(|label| {
        app.get_webview_window(label)
            .is_some_and(|window| window.is_visible().unwrap_or(false))
    });
    let _ = app.emit_to("mascot", "chat-visible", visible);
}

#[tauri::command]
pub fn close_chat(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("chat") {
        window.hide().map_err(|e| e.to_string())?;
    }
    notify_chat_visibility(&app);
    Ok(())
}

pub fn show_panel(app: &tauri::AppHandle, label: &str, cursor: bool) -> Result<(), String> {
    let window = if let Some(w) = app.get_webview_window(label) {
        w
    } else {
        let (title, width, height) = match label {
            "chat" => ("Michiyo · Chat", 320., 44.),
            "translate" => ("Michiyo · Dịch", 420., 400.),
            "settings" => ("Michiyo · Cài đặt", 720., 540.),
            "menu" => ("Michiyo · Menu", 156., 190.),
            _ => return Err("Cửa sổ không hợp lệ".into()),
        };
        tauri::WebviewWindowBuilder::new(
            app,
            label,
            tauri::WebviewUrl::App(format!("index.html?view={label}").into()),
        )
        .title(title)
        .inner_size(width, height)
        .visible(false)
        .always_on_top(label != "settings")
        .decorations(!matches!(label, "chat" | "menu"))
        .transparent(matches!(label, "chat" | "menu"))
        .shadow(!matches!(label, "chat" | "menu"))
        .skip_taskbar(matches!(label, "chat" | "menu"))
        .resizable(!matches!(label, "chat" | "menu"))
        .build()
        .map_err(|e| e.to_string())?
    };
    position_panel(app, &window, cursor || label == "menu")?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    if label == "chat" {
        notify_chat_visibility(app);
    }
    if label == "menu" {
        let _ = app.emit_to("menu", "menu-focus", ());
    }
    if label == "chat" {
        let _ = app.emit_to("chat", "chat-focus", ());
    }
    Ok(())
}

#[tauri::command]
pub fn resize_chat(app: tauri::AppHandle, height: f64) -> Result<f64, String> {
    if !height.is_finite() {
        return Err("Chiều cao không hợp lệ".into());
    }
    if let Some(window) = app.get_webview_window("chat") {
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let max_height = window
            .current_monitor()
            .map_err(|e| e.to_string())?
            .map(|monitor| monitor.work_area().size.height as f64 / scale)
            .unwrap_or(720.)
            .max(40.);
        let height = height.clamp(40., max_height);
        let size = window.inner_size().map_err(|e| e.to_string())?;
        if (size.height as f64 - height * scale).abs() > 1.
            || (size.width as f64 - 320. * scale).abs() > 1.
        {
            window
                .set_size(tauri::LogicalSize::new(320., height))
                .map_err(|e| e.to_string())?;
        }
        position_panel(&app, &window, false)?;
        return Ok(max_height);
    }
    Ok(720.)
}

#[tauri::command]
pub async fn mascot_menu(app: tauri::AppHandle) -> Result<(), String> {
    show_panel(&app, "menu", true)
}

#[tauri::command]
pub async fn menu_action(app: tauri::AppHandle, action: String) -> Result<(), String> {
    if let Some(menu) = app.get_webview_window("menu") {
        let _ = menu.hide();
    }
    match action.as_str() {
        "main" => open_main_impl(app, "chat".into()),
        "chat" | "translate" | "settings" => open_main_impl(app, action),
        "hide" => {
            if let Some(w) = app.get_webview_window("mascot") {
                w.hide().map_err(|e| e.to_string())?;
                let _ = app.emit_to("mascot", "mascot-visible", false);
            }
            if let Some(w) = app.get_webview_window("chat") {
                let _ = w.hide();
            }
            notify_chat_visibility(&app);
            Ok(())
        }
        "quit" => {
            app.exit(0);
            Ok(())
        }
        _ => Err("Thao tác không hợp lệ".into()),
    }
}

pub(crate) fn position_panel(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    cursor: bool,
) -> Result<(), String> {
    let mascot = app
        .get_webview_window("mascot")
        .ok_or("Không tìm thấy mascot")?;
    let point = if cursor {
        mascot.cursor_position().map_err(|e| e.to_string())?
    } else {
        let p = mascot.outer_position().map_err(|e| e.to_string())?;
        tauri::PhysicalPosition::new(p.x as f64, p.y as f64)
    };
    let monitor = app
        .monitor_from_point(point.x, point.y)
        .map_err(|e| e.to_string())?
        .or(mascot.current_monitor().map_err(|e| e.to_string())?);
    if let Some(m) = monitor {
        let area = m.work_area();
        let origin = &area.position;
        let size = &area.size;
        let ws = window.outer_size().map_err(|e| e.to_string())?;
        if window.label() == "menu" {
            let gap = 4. * m.scale_factor();
            let x = (point.x + gap).clamp(
                origin.x as f64,
                (origin.x as f64 + size.width as f64 - ws.width as f64).max(origin.x as f64),
            );
            let y = (point.y + gap).clamp(
                origin.y as f64,
                (origin.y as f64 + size.height as f64 - ws.height as f64).max(origin.y as f64),
            );
            window
                .set_position(tauri::PhysicalPosition::new(x as i32, y as i32))
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        if window.label() == "chat" && !cursor {
            let mascot_size = mascot.inner_size().map_err(|e| e.to_string())?;
            let state = app.state::<Runtime>();
            let mask = state.mask.lock().unwrap();
            let mut left = 160;
            let mut right = 0;
            let mut bottom = 0;
            for (index, opaque) in mask.iter().enumerate() {
                if *opaque {
                    left = left.min(index % 160);
                    right = right.max(index % 160 + 1);
                    bottom = bottom.max(index / 160 + 1);
                }
            }
            if bottom == 0 {
                left = 48;
                right = 112;
                bottom = 133;
            }
            let pet_left = point.x + left as f64 * mascot_size.width as f64 / 160.;
            let pet_right = point.x + right as f64 * mascot_size.width as f64 / 160.;
            let pet_bottom = point.y + bottom as f64 * mascot_size.height as f64 / 160.;
            let gap = 4. * m.scale_factor();
            let opens_right =
                pet_right + gap + ws.width as f64 <= origin.x as f64 + size.width as f64;
            let x = if opens_right {
                pet_right + gap
            } else {
                pet_left - gap - ws.width as f64
            };
            let y = pet_bottom - ws.height as f64;
            window
                .set_position(tauri::PhysicalPosition::new(
                    x.clamp(
                        origin.x as f64,
                        (origin.x as f64 + size.width as f64 - ws.width as f64)
                            .max(origin.x as f64),
                    ) as i32,
                    y.clamp(
                        origin.y as f64,
                        (origin.y as f64 + size.height as f64 - ws.height as f64)
                            .max(origin.y as f64),
                    ) as i32,
                ))
                .map_err(|e| e.to_string())?;
            let _ = app.emit_to(
                "chat",
                "chat-placement",
                if opens_right { "right" } else { "left" },
            );
            return Ok(());
        }
        let x = if point.x
            + mascot.outer_size().map_err(|e| e.to_string())?.width as f64
            + 12.
            + ws.width as f64
            > (origin.x as f64 + size.width as f64)
        {
            point.x - ws.width as f64 - 12.
        } else {
            point.x + mascot.outer_size().map_err(|e| e.to_string())?.width as f64 + 12.
        };
        let y = point
            .y
            .min(origin.y as f64 + size.height as f64 - ws.height as f64)
            .max(origin.y as f64);
        let _ = window.set_position(tauri::PhysicalPosition::new(
            x.max(origin.x as f64) as i32,
            y as i32,
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn open_panel(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if !["chat", "settings", "translate"].contains(&label.as_str()) {
        return Err("Cửa sổ không hợp lệ".into());
    }
    if label == "chat" {
        return open_main_impl(app, "chat".into());
    }
    show_panel(&app, &label, false)
}

pub fn save_mascot_position(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let w = app
        .get_webview_window("mascot")
        .ok_or("Không tìm thấy mascot")?;
    let p = w.outer_position().map_err(|e| e.to_string())?;
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set("position", json!({"x":p.x,"y":p.y}));
    store.save().map_err(|e| e.to_string())?;
    if let Some(chat) = app.get_webview_window("chat") {
        if chat.is_visible().unwrap_or(false) {
            position_panel(&app, &chat, false)?;
        }
    }
    Ok(())
}

pub async fn capture_selection(app: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    if app
        .state::<Runtime>()
        .capturing
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let result = tauri::async_runtime::spawn_blocking(|| -> Result<String, String> {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};
        std::thread::sleep(std::time::Duration::from_millis(180));
        let mut cb = arboard::Clipboard::new().map_err(|_| "Không mở được clipboard")?;
        let old = cb.get_text().ok();
        let image = cb.get_image().ok();
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|_| "Không giả lập được bàn phím")?;
        let marker = format!("michiyo-selection-{:?}", std::time::SystemTime::now());
        cb.set_text(&marker).map_err(|_| "Clipboard đang bận")?;
        let selected = (|| -> Result<String, String> {
            let modifier = if cfg!(target_os = "macos") {
                Key::Meta
            } else {
                Key::Control
            };
            let _ = enigo.key(Key::Alt, Direction::Release);
            enigo
                .key(modifier, Direction::Press)
                .map_err(|e| e.to_string())?;
            let copied = enigo.key(Key::Unicode('c'), Direction::Click);
            let _ = enigo.key(modifier, Direction::Release);
            copied.map_err(|e| e.to_string())?;
            for _ in 0..12 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Ok(text) = cb.get_text() {
                    if text != marker && !text.trim().is_empty() {
                        return Ok(text);
                    }
                }
            }
            Err("Không có văn bản được chọn. Hãy bôi đen văn bản rồi thử lại.".into())
        })();
        if let Some(img) = image {
            cb.set_image(img)
                .map_err(|_| "Không khôi phục được clipboard ảnh")?;
        } else if let Some(text) = old {
            cb.set_text(text)
                .map_err(|_| "Không khôi phục được clipboard")?;
        } else {
            let _ = cb.clear();
        }
        selected
    })
    .await;
    app.state::<Runtime>()
        .capturing
        .store(false, Ordering::SeqCst);
    match result {
        Ok(Ok(text)) => {
            let _ = show_panel(&app, "translate", true);
            *app.state::<Runtime>().pending_capture_error.lock().unwrap() = None;
            *app.state::<Runtime>().pending_selection.lock().unwrap() = Some(text);
            let _ = app.emit_to("translate", "selection", ());
        }
        other => {
            let message = match other {
                Ok(Err(e)) => e,
                _ => "Không lấy được văn bản".into(),
            };
            let _ = show_panel(&app, "translate", true);
            *app.state::<Runtime>().pending_capture_error.lock().unwrap() = Some(message);
            let _ = app.emit_to("translate", "selection", ());
            let _ = app.emit("mascot-state", "error");
        }
    }
}

fn register_shortcuts(
    app: &tauri::AppHandle,
    keys: &[(String, &'static str)],
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let manager = app.global_shortcut();
    for (key, action) in keys {
        let handle = app.clone();
        let action = *action;
        manager
            .on_shortcut(key.as_str(), move |_, _, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }
                match action {
                    "translate" => {
                        let h = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            capture_selection(h).await;
                        });
                    }
                    "main" | "chat" => {
                        let h = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = open_main(h, "chat".into()).await;
                        });
                    }
                    _ => {
                        if let Some(w) = handle.get_webview_window("mascot") {
                            let visible = !w.is_visible().unwrap_or(false);
                            if visible {
                                let _ = w.show();
                            } else {
                                let _ = w.hide();
                            }
                            let _ = handle.emit_to("mascot", "mascot-visible", visible);
                        }
                    }
                }
            })
            .map_err(|e| format!("Phím tắt {key} bị trùng hoặc không hợp lệ: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn apply_config(app: tauri::AppHandle) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let c = config::read(&app)?;
    let state = app.state::<Runtime>();
    state.clipboard_watch.store(
        c["clipboardWatch"].as_bool() == Some(true),
        Ordering::SeqCst,
    );
    let shortcuts = vec![
        (
            config::string(&c, "translateShortcut", "Ctrl+Alt+T"),
            "translate",
        ),
        (config::string(&c, "chatShortcut", "Ctrl+Alt+C"), "chat"),
        (config::string(&c, "toggleShortcut", "Ctrl+Alt+M"), "toggle"),
        (
            config::string(&c, "shellShortcut", "Ctrl+Alt+Space"),
            "main",
        ),
    ];
    let parsed = shortcuts
        .iter()
        .map(|(key, _)| key.parse::<Shortcut>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if parsed
        .iter()
        .map(|s| s.id())
        .collect::<std::collections::HashSet<_>>()
        .len()
        != shortcuts.len()
    {
        return Err("Các phím tắt không được trùng nhau".into());
    }
    let previous = state.shortcuts.lock().unwrap().clone();
    if previous != shortcuts {
        app.global_shortcut()
            .unregister_all()
            .map_err(|e| e.to_string())?;
        if let Err(error) = register_shortcuts(&app, &shortcuts) {
            let _ = app.global_shortcut().unregister_all();
            let _ = register_shortcuts(&app, &previous);
            return Err(error);
        }
        *state.shortcuts.lock().unwrap() = shortcuts;
    }
    if let Some(w) = app.get_webview_window("mascot") {
        let scale = c["scale"].as_f64().unwrap_or(1.).clamp(1., 3.);
        w.set_size(tauri::LogicalSize::new(160. * scale, 160. * scale))
            .map_err(|e| e.to_string())?;
    }
    let anchor = config::string(&c, "anchor", "bottom-right");
    let previous_anchor = state.anchor.lock().unwrap().replace(anchor.clone());
    if previous_anchor.is_some_and(|old| old != anchor) {
        reset_position(app.clone())?;
    }
    ensure_mascot_on_screen(&app)?;
    let _ = app.emit("config-changed", ());
    Ok(())
}

// Saved physical coordinates may belong to a monitor that is no longer attached.
// Validate after applying scale so the complete mascot remains reachable.
pub fn ensure_mascot_on_screen(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("mascot") else {
        return Ok(());
    };
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let visible = monitors.iter().any(|monitor| {
        let area = monitor.work_area();
        position.x as i64 >= area.position.x as i64
            && position.y as i64 >= area.position.y as i64
            && position.x as i64 + size.width as i64
                <= area.position.x as i64 + area.size.width as i64
            && position.y as i64 + size.height as i64
                <= area.position.y as i64 + area.size.height as i64
    });
    if !visible && !monitors.is_empty() {
        reset_position(app.clone())?;
        save_mascot_position(app.clone())?;
    }
    Ok(())
}

#[tauri::command]
pub fn take_selection(state: tauri::State<'_, Runtime>) -> Value {
    if let Some(text) = state.pending_selection.lock().unwrap().take() {
        json!({"text":text})
    } else if let Some(error) = state.pending_capture_error.lock().unwrap().take() {
        json!({"error":error})
    } else {
        Value::Null
    }
}

#[tauri::command]
pub fn copy_text(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    text: String,
) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    *state.ignored_clipboard.lock().unwrap() = Some(text.clone());
    app.clipboard()
        .write_text(text)
        .map_err(|_| "Không ghi được clipboard".into())
}

#[tauri::command]
pub fn take_chat_prompt(state: tauri::State<'_, Runtime>) -> Option<String> {
    state.pending_chat.lock().unwrap().take()
}

#[tauri::command]
pub async fn ask_translation(app: tauri::AppHandle, text: String) -> Result<(), String> {
    *app.state::<Runtime>().pending_chat.lock().unwrap() = Some(text);
    show_panel(&app, "chat", false)?;
    let _ = app.emit_to("chat", "ask-translation", ());
    Ok(())
}

#[tauri::command]
pub fn reset_position(app: tauri::AppHandle) -> Result<(), String> {
    let c = config::read(&app)?;
    let anchor = config::string(&c, "anchor", "bottom-right");
    let w = app.get_webview_window("mascot").ok_or("Không có mascot")?;
    if let Some(m) = w
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or(w.primary_monitor().map_err(|e| e.to_string())?)
    {
        let area = m.work_area();
        let size = w.outer_size().map_err(|e| e.to_string())?;
        let x = area.position.x
            + if anchor.ends_with("right") {
                (area.size.width.saturating_sub(size.width)) as i32
            } else {
                0
            };
        let y = area.position.y
            + if anchor.starts_with("bottom") {
                (area.size.height.saturating_sub(size.height)) as i32
            } else {
                0
            };
        w.set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
