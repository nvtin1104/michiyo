pub mod skins;
use crate::{
    config,
    providers::{Provider, Translation, Translator},
    secrets,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};
pub struct Runtime {
    pub client: reqwest::Client,
    pub cache: Mutex<lru::LruCache<String, Translation>>,
    pub jobs: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
    pub mask: Mutex<Vec<bool>>,
    pub shortcuts: Mutex<Vec<(String, &'static str)>>,
    pub capturing: std::sync::atomic::AtomicBool,
    pub pending_selection: Mutex<Option<String>>,
    pub pending_capture_error: Mutex<Option<String>>,
    pub ignored_clipboard: Mutex<Option<String>>,
    pub pending_chat: Mutex<Option<String>>,
    pub anchor: Mutex<Option<String>>,
    pub move_generation: std::sync::atomic::AtomicU64,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
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
            pending_capture_error: Mutex::new(None),
            ignored_clipboard: Mutex::new(None),
            pending_chat: Mutex::new(None),
            anchor: Mutex::new(None),
            move_generation: std::sync::atomic::AtomicU64::new(0),
        }
    }
}
#[tauri::command]
pub fn set_hit_mask(state: tauri::State<'_, Runtime>, mask: Vec<bool>) -> Result<(), String> {
    if mask.len() != 160 * 160 {
        return Err("Sai kích thước hit mask".into());
    }
    *state.mask.lock().unwrap() = mask;
    Ok(())
}
#[tauri::command]
pub fn save_secret(app: tauri::AppHandle, name: String, value: String) -> Result<(), String> {
    secrets::set(&name, &value)?;
    let _ = app.emit("secrets-changed", ());
    Ok(())
}
#[tauri::command]
pub fn secret_status() -> Value {
    json!({"claude":secrets::get("claude").is_ok(),"google":secrets::get("google").is_ok(),"libre":secrets::get("libre").is_ok()})
}
#[tauri::command]
pub async fn translate(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    text: String,
    engine: Option<String>,
) -> Result<Translation, String> {
    if text.trim().is_empty() || text.chars().count() > 10000 {
        return Err("Hãy chọn từ 1 đến 10.000 ký tự".into());
    }
    let c = config::read(&app)?;
    let detected = whatlang::detect(&text)
        .map(|i| {
            if i.lang() == whatlang::Lang::Vie {
                "vi"
            } else {
                isolang::Language::from_639_3(i.lang().code())
                    .and_then(|l| l.to_639_1())
                    .unwrap_or("en")
            }
        })
        .unwrap_or("en");
    let src = config::string(&c, "source", "auto");
    let src = if src == "auto" {
        detected.to_string()
    } else {
        src
    };
    let target = config::string(&c, "target", "vi");
    let tgt = if src == target {
        config::string(&c, "alternate", "en")
    } else {
        target
    };
    let key = format!("{text}|{src}|{tgt}|{c}|{engine:?}");
    if let Some(result) = state.cache.lock().unwrap().get(&key) {
        return Ok(result.clone());
    }
    let names = engine.map(|e| vec![e]).unwrap_or_else(|| {
        c["providers"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or(vec!["mymemory".into()])
    });
    let mut errors = vec![];
    let _ = app.emit("mascot-state", "thinking");
    for name in names {
        let p = Provider {
            name: name.clone(),
            client: state.client.clone(),
            base: None,
            app: Some(app.clone()),
        };
        match p.translate(&text, &src, &tgt, &c).await {
            Ok(output) => {
                let result = Translation {
                    text: output,
                    engine: name,
                    source: src,
                    target: tgt,
                };
                state.cache.lock().unwrap().put(key, result.clone());
                let _ = app.emit("mascot-state", "happy");
                return Ok(result);
            }
            Err(e) => errors.push(format!("{}: {}", name, e)),
        }
        if c["fallback"].as_bool() == Some(false) {
            break;
        }
    }
    let _ = app.emit("mascot-state", "error");
    Err(errors.join(" · "))
}
#[tauri::command]
pub async fn chat(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
    messages: Value,
) -> Result<(), String> {
    let cancel = Arc::new(tokio::sync::Notify::new());
    {
        let mut jobs = state.jobs.lock().unwrap();
        if !jobs.is_empty() {
            return Err("Một câu trả lời đang được tạo".into());
        }
        jobs.insert(id.clone(), cancel.clone());
    }
    let outcome = async {
        let c = config::read(&app)?;
        let _=app.emit("mascot-state","thinking");
        if !messages.is_array()||messages.as_array().is_some_and(|a|a.is_empty()||a.len()>1000)||messages.to_string().len()>1_000_000{return Err("Hội thoại quá dài. Hãy tạo cuộc trò chuyện mới.".into())}
        let request=json!({"action":"chat","model":config::string(&c,"model","claude-haiku-5-5"),
            "system":config::string(&c,"systemPrompt","You are Michiyo, a friendly conversational companion."),
            "maxTokens":c["maxTokens"].as_u64().unwrap_or(2048).clamp(1,16384),"messages":messages});
        crate::providers::agent::run(&app,request,cancel.clone(),|text|{
            let _=app.emit_to("chat","chat-delta",json!({"id":id,"text":text}));
            let _=app.emit("mascot-state","talking");
        }).await.map(|_|())
    }
    .await;
    state.jobs.lock().unwrap().remove(&id);
    let _ = app.emit(
        "mascot-state",
        if outcome.is_ok() { "happy" } else { "error" },
    );
    outcome
}
#[tauri::command]
pub fn stop_chat(state: tauri::State<'_, Runtime>, id: String) {
    if let Some(job) = state.jobs.lock().unwrap().get(&id) {
        job.notify_one();
    }
}
#[tauri::command]
pub async fn test_connection(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    name: String,
) -> Result<String, String> {
    if name == "claude" {
        let c = config::read(&app)?;
        crate::providers::agent::run(&app,json!({"action":"check","model":config::string(&c,"model","claude-haiku-5-5"),"system":"Reply only OK.","maxTokens":32,"messages":[{"role":"user","content":"Reply OK"}]}),Arc::new(tokio::sync::Notify::new()),|_|{}).await?;
    } else {
        let p = Provider {
            name,
            client: state.client.clone(),
            base: None,
            app: Some(app.clone()),
        };
        p.translate("Hello", "en", "vi", &config::read(&app)?)
            .await?;
    }
    Ok("Kết nối thành công".into())
}
pub fn show_panel(app: &tauri::AppHandle, label: &str, cursor: bool) -> Result<(), String> {
    let window = if let Some(w) = app.get_webview_window(label) {
        w
    } else {
        let (title, width, height) = match label {
            "chat" => ("Michiyo · Chat", 320., 44.),
            "translate" => ("Michiyo · Dịch", 420., 400.),
            "settings" => ("Michiyo · Cài đặt", 720., 540.),
            "menu" => ("Michiyo · Menu", 156., 160.),
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
    if label == "menu" {
        let _ = app.emit_to("menu", "menu-focus", ());
    }
    if label == "chat" {
        let _ = app.emit_to("chat", "chat-focus", ());
    }
    Ok(())
}
#[tauri::command]
pub fn resize_chat(app: tauri::AppHandle, height: f64) -> Result<(), String> {
    if !height.is_finite() {
        return Err("Chiều cao không hợp lệ".into());
    }
    if let Some(window) = app.get_webview_window("chat") {
        let height = height.clamp(40., 304.);
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let size = window.inner_size().map_err(|e| e.to_string())?;
        if (size.height as f64 - height * scale).abs() > 1.
            || (size.width as f64 - 320. * scale).abs() > 1.
        {
            window
                .set_size(tauri::LogicalSize::new(320., height))
                .map_err(|e| e.to_string())?;
        }
        position_panel(&app, &window, false)?;
    }
    Ok(())
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
        "chat" | "translate" | "settings" => show_panel(&app, &action, false),
        "hide" => {
            if let Some(w) = app.get_webview_window("mascot") {
                w.hide().map_err(|e| e.to_string())?;
                let _ = app.emit_to("mascot", "mascot-visible", false);
            }
            if let Some(w) = app.get_webview_window("chat") {
                let _ = w.hide();
            }
            Ok(())
        }
        "quit" => {
            app.exit(0);
            Ok(())
        }
        _ => Err("Thao tác không hợp lệ".into()),
    }
}
fn position_panel(
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
            // Attach to visible sprite pixels, not the transparent 160×160 canvas margin.
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
            let x = if pet_right + gap + ws.width as f64 <= origin.x as f64 + size.width as f64 {
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
                    "chat" => {
                        let h = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = show_panel(&h, "chat", false);
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
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let c = config::read(&app)?;
    let shortcuts = vec![
        (
            config::string(&c, "translateShortcut", "Ctrl+Alt+T"),
            "translate",
        ),
        (config::string(&c, "chatShortcut", "Ctrl+Alt+C"), "chat"),
        (config::string(&c, "toggleShortcut", "Ctrl+Alt+M"), "toggle"),
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
        != 3
    {
        return Err("Các phím tắt không được trùng nhau".into());
    }
    let state = app.state::<Runtime>();
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
    let Some(window) = app.get_webview_window("mascot") else { return Ok(()); };
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let visible = monitors.iter().any(|monitor| {
        let area = monitor.work_area();
        position.x as i64 >= area.position.x as i64
            && position.y as i64 >= area.position.y as i64
            && position.x as i64 + size.width as i64 <= area.position.x as i64 + area.size.width as i64
            && position.y as i64 + size.height as i64 <= area.position.y as i64 + area.size.height as i64
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
