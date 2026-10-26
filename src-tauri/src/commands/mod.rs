pub mod skins;
use crate::{
    config,
    providers::{Provider, Translation, Translator},
    secrets,
};
use futures_util::StreamExt;
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
        let key = secrets::get("claude")?;
        let _ = app.emit("mascot-state", "thinking");
        let request = state
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model":config::string(&c,"model","claude-haiku-5-5"),
                "system":config::string(&c,"systemPrompt","You are Michiyo."),
                "max_tokens":c["maxTokens"].as_u64().unwrap_or(2048).clamp(1,16384),
                "messages":messages,"stream":true
            }))
            .send();
        let response = tokio::select! {
            _=cancel.notified()=>return Ok(()),
            r=request=>r.map_err(|_|"Không kết nối được Claude")?
        };
        if !response.status().is_success() {
            return Err(format!(
                "Claude HTTP {}. Kiểm tra API key, model và hạn mức.",
                response.status().as_u16()
            ));
        }
        let mut stream = response.bytes_stream();
        let mut decoder = crate::providers::sse::Decoder::default();
        loop {
            let chunk =
                tokio::select! {_=cancel.notified()=>return Ok(()),chunk=stream.next()=>chunk};
            let Some(chunk) = chunk else {
                return Err("Claude đóng kết nối trước khi hoàn tất. Có thể thử gửi lại.".into());
            };
            for event in decoder.push(&chunk.map_err(|_| "Kết nối streaming bị gián đoạn")?)?
            {
                match event["type"].as_str() {
                    Some("error") => return Err("Claude báo lỗi trong luồng phản hồi".into()),
                    Some("message_stop") => return Ok(()),
                    Some("content_block_delta") => {
                        if let Some(text) = event["delta"]["text"].as_str() {
                            let _ = app.emit_to("chat", "chat-delta", json!({"id":id,"text":text}));
                            let _ = app.emit("mascot-state", "talking");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    .await;
    state.jobs.lock().unwrap().remove(&id);
    let _ = app.emit(
        "mascot-state",
        if outcome.is_ok() { "idle" } else { "error" },
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
        let response = state
            .client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", secrets::get("claude")?)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|_| "Không kết nối được Claude")?;
        crate::providers::checked(response).await?;
    } else {
        let p = Provider {
            name,
            client: state.client.clone(),
            base: None,
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
            "chat" => ("Michiyo · Chat", 420., 600.),
            "translate" => ("Michiyo · Dịch", 420., 400.),
            "settings" => ("Michiyo · Cài đặt", 860., 720.),
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
        .build()
        .map_err(|e| e.to_string())?
    };
    position_panel(app, &window, cursor)?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
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
#[tauri::command]
pub fn snap_mascot(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let w = app.get_webview_window("mascot").unwrap();
    let p = w.outer_position().map_err(|e| e.to_string())?;
    if let Some(m) = w
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or(w.primary_monitor().map_err(|e| e.to_string())?)
    {
        let area = m.work_area();
        let o = &area.position;
        let s = &area.size;
        let ws = w.outer_size().map_err(|e| e.to_string())?;
        let right = o.x + s.width as i32 - ws.width as i32;
        let bottom = o.y + s.height as i32 - ws.height as i32;
        let distances = [
            (p.x - o.x).abs(),
            (right - p.x).abs(),
            (p.y - o.y).abs(),
            (bottom - p.y).abs(),
        ];
        let edge = distances
            .iter()
            .enumerate()
            .min_by_key(|(_, d)| *d)
            .unwrap()
            .0;
        let mut x = p.x.clamp(o.x, right.max(o.x));
        let mut y = p.y.clamp(o.y, bottom.max(o.y));
        match edge {
            0 => x = o.x,
            1 => x = right,
            2 => y = o.y,
            _ => y = bottom,
        }
        if p.x != x || p.y != y {
            w.set_position(tauri::PhysicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
        }
        let store = app.store("settings.json").map_err(|e| e.to_string())?;
        store.set("position", json!({"x":x,"y":y}));
        store.save().map_err(|e| e.to_string())?;
    }
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
    let _ = app.emit("config-changed", ());
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
