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
    pub history_lock: Mutex<()>,
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
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            history_lock: Mutex::new(()),
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
        }
    }
}
fn record_activity(app: &tauri::AppHandle, state: &Runtime, kind: &str, data: Value) {
    if let Ok(_guard) = state.history_lock.lock() {
        if let Ok(mut db) = local_db(app) {
            if crate::db::activity::record(&mut db, kind, data).is_ok() {
                let _ = app.emit("activity-changed", ());
            }
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
    json!({"claude":secrets::get("claude").is_ok(),"google":secrets::get("google").is_ok(),"libre":secrets::get("libre").is_ok(),"openai":secrets::get("openai").is_ok(),"cloud":secrets::get("cloud").is_ok()})
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
    let cached = state.cache.lock().unwrap().get(&key).cloned();
    if let Some(result) = cached {
        record_activity(
            &app,
            &state,
            "translation",
            json!({"engine": result.engine}),
        );
        return Ok(result);
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
    let names: Vec<String> = names
        .into_iter()
        .filter(|name| crate::ai::translator_allowed(&c, name))
        .collect();
    if names.is_empty() {
        return Err("Chế độ Offline: không có engine dịch chạy trên máy này. Hãy dùng LibreTranslate trên localhost hoặc đổi chế độ.".into());
    }
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
                record_activity(
                    &app,
                    &state,
                    "translation",
                    json!({"engine": result.engine}),
                );
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
    window: tauri::WebviewWindow,
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
        let backends = crate::ai::plan(&c)?;
        let system = config::string(&c,"systemPrompt","You are Michiyo, a friendly conversational companion.");
        let max_tokens = c["maxTokens"].as_u64().unwrap_or(2048).clamp(1,16384);
        let mut last_error = String::new();
        let mut sent_any = false;
        for backend in backends {
            let mut on_delta = |text: &str| {
                sent_any = true;
                let _=app.emit_to(window.label(),"chat-delta",json!({"id":id,"text":text}));
                let _=app.emit("mascot-state","talking");
            };
            let result = match &backend {
                crate::ai::Backend::Claude{..} => crate::providers::agent::run(&app,request.clone(),cancel.clone(),|text|on_delta(text)).await.map(|_|()),
                crate::ai::Backend::OpenAi{base,model,use_key} => {
                    let key = if *use_key { secrets::get("openai").ok() } else { None };
                    crate::ai::stream_chat(&state.client,base,key.as_deref(),model,&system,&messages,max_tokens,cancel.clone(),|text|on_delta(text)).await
                }
                crate::ai::Backend::Cloudflare{base,model} => {
                    let key = secrets::get("cloud").ok();
                    crate::ai::stream_chat(&state.client,base,key.as_deref(),model,&system,&messages,max_tokens,cancel.clone(),|text|on_delta(text)).await
                }
            };
            match result {
                Ok(()) => return Ok(()),
                // Falling back mid-answer would splice two models together.
                Err(e) if sent_any => return Err(e),
                Err(e) => last_error = e,
            }
        }
        Err(last_error)
    }
    .await;
    if outcome.is_ok() {
        record_activity(&app, &state, "chat", json!({}));
    }
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
/// One-shot completion through the configured router (same mode rules as chat).
async fn complete_once(
    app: &tauri::AppHandle,
    state: &Runtime,
    system: &str,
    user: &str,
    max_tokens: u64,
) -> Result<String, String> {
    let c = config::read(app)?;
    let messages = json!([{"role": "user", "content": user}]);
    let mut last_error = String::new();
    for backend in crate::ai::plan(&c)? {
        let mut out = String::new();
        let cancel = Arc::new(tokio::sync::Notify::new());
        let result = match &backend {
            crate::ai::Backend::Claude { model } => crate::providers::agent::run(
                app,
                json!({"action":"chat","model":model,"system":system,"maxTokens":max_tokens,"messages":messages}),
                cancel,
                |text| out.push_str(text),
            )
            .await
            .map(|full| {
                if out.is_empty() {
                    out = full;
                }
            }),
            crate::ai::Backend::OpenAi { base, model, use_key } => {
                let key = if *use_key { secrets::get("openai").ok() } else { None };
                crate::ai::stream_chat(&state.client, base, key.as_deref(), model, system, &messages, max_tokens, cancel, |text| out.push_str(text)).await
            }
            crate::ai::Backend::Cloudflare { base, model } => {
                let key = secrets::get("cloud").ok();
                crate::ai::stream_chat(&state.client, base, key.as_deref(), model, system, &messages, max_tokens, cancel, |text| out.push_str(text)).await
            }
        };
        match result {
            Ok(()) if !out.trim().is_empty() => return Ok(out),
            Ok(()) => last_error = "Model không trả về nội dung".into(),
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}
#[tauri::command]
pub async fn ai_summarize(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    text: String,
) -> Result<String, String> {
    if text.trim().is_empty() || text.chars().count() > 20_000 {
        return Err("Hãy chọn nội dung từ 1 đến 20.000 ký tự".into());
    }
    complete_once(&app, &state, crate::ai::actions::SUMMARY_SYSTEM, &text, 600).await
}
/// `now_local` is the UI's local time (`YYYY-MM-DD HH:MM`) so relative dates like "mai" resolve correctly.
#[tauri::command]
pub async fn ai_suggest_actions(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    text: String,
    now_local: String,
) -> Result<Vec<crate::ai::actions::Suggestion>, String> {
    if text.trim().is_empty() || text.chars().count() > 8_000 {
        return Err("Hãy chọn nội dung từ 1 đến 8.000 ký tự".into());
    }
    if now_local.len() != 16 || !now_local.is_ascii() {
        return Err("Thời gian không hợp lệ".into());
    }
    let raw = complete_once(&app, &state, &crate::ai::actions::suggest_system(&now_local), &text, 800).await?;
    Ok(crate::ai::actions::parse_suggestions(&raw))
}
/// Exchanges a pairing code from the admin site for a device token kept in the OS keychain.
#[tauri::command]
pub async fn cloud_pair(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    url: String,
    code: String,
) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/').to_string();
    crate::ai::validate_base(&url)?;
    if code.trim().is_empty() || code.len() > 32 {
        return Err("Hãy nhập mã ghép nối".into());
    }
    let response = state
        .client
        .post(format!("{url}/v1/pair/complete"))
        .json(&json!({"code": code.trim(), "device_name": "Michiyo Desktop"}))
        .send()
        .await
        .map_err(|_| "Không kết nối được Michiyo Cloud. Hãy kiểm tra địa chỉ.".to_string())?;
    match response.status().as_u16() {
        200 => {}
        400 => return Err("Mã ghép nối sai hoặc đã hết hạn".into()),
        429 => return Err("Thử quá nhiều lần. Hãy đợi vài phút.".into()),
        code => return Err(format!("Michiyo Cloud trả về HTTP {code}")),
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Phản hồi từ Michiyo Cloud không hợp lệ".to_string())?;
    let token = body["token"].as_str().filter(|t| t.starts_with("md_")).ok_or("Phản hồi từ Michiyo Cloud không hợp lệ")?;
    secrets::set("cloud", token)?;
    let _ = app.emit("secrets-changed", ());
    Ok(body["device_id"].as_str().unwrap_or_default().to_string())
}
#[tauri::command]
pub async fn ai_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    target: String,
) -> Result<Vec<String>, String> {
    let c = config::read(&app)?;
    let (base, key) = match target.as_str() {
        "local" => (config::string(&c, "localBase", "http://127.0.0.1:11434/v1"), None),
        "external" => (
            config::string(&c, "openaiBase", "https://api.openai.com/v1"),
            secrets::get("openai").ok(),
        ),
        _ => return Err("Nguồn model không hợp lệ".into()),
    };
    crate::ai::list_models(&state.client, &base, key.as_deref()).await
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
#[tauri::command]
pub fn open_chat_in_main(
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
    open_main(app, "chat".into())
}

#[tauri::command]
pub fn take_shell_chat_draft(state: tauri::State<'_, Runtime>) -> Option<Value> {
    state.pending_shell_chat_draft.lock().ok()?.take()
}

#[tauri::command]
pub fn open_translation_in_main(
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
    open_main(app, "translate".into())
}

#[tauri::command]
pub fn take_translation_draft(state: tauri::State<'_, Runtime>) -> Option<Value> {
    state.pending_translation_draft.lock().ok()?.take()
}

#[tauri::command]
pub fn open_main(app: tauri::AppHandle, route: String) -> Result<(), String> {
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
    let window = if let Some(window) = app.get_webview_window("main") {
        window
            .emit("shell-navigate", &route)
            .map_err(|e| e.to_string())?;
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
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?
    };
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
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
        "main" => open_main(app, "home".into()),
        "chat" | "translate" | "settings" => open_main(app, action),
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
                    "main" => {
                        let _ = open_main(handle.clone(), "home".into());
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

pub(crate) fn local_db(app: &tauri::AppHandle) -> Result<rusqlite::Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Không tìm thấy thư mục dữ liệu")?;
    std::fs::create_dir_all(&dir).map_err(|_| "Không tạo được thư mục dữ liệu")?;
    crate::db::open(&dir.join("michiyo.db")).map_err(|_| "Không mở được cơ sở dữ liệu".to_string())
}

fn history_db(app: &tauri::AppHandle) -> Result<rusqlite::Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Không tìm thấy thư mục dữ liệu")?;
    std::fs::create_dir_all(&dir).map_err(|_| "Không tạo được thư mục dữ liệu")?;
    let mut db = crate::db::open(&dir.join("michiyo.db"))
        .map_err(|_| "Không mở được cơ sở dữ liệu lịch sử")?;
    crate::db::import_history(&mut db, &dir.join("history.json")).map_err(|_| {
        "Không nhập được lịch sử cũ. Dữ liệu gốc vẫn được giữ; hãy kiểm tra history.json."
    })?;
    Ok(db)
}

#[tauri::command]
pub fn history_list(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<Vec<crate::db::Conversation>, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không mở được lịch sử")?;
    let db = history_db(&app)?;
    crate::db::list_history(&db).map_err(|_| "Không đọc được lịch sử".to_string())
}

#[tauri::command]
pub fn history_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    conversation: crate::db::Conversation,
    limit: usize,
) -> Result<Vec<crate::db::Conversation>, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không lưu được lịch sử")?;
    let mut db = history_db(&app)?;
    let result = crate::db::save_history(&mut db, &conversation, limit)
        .map_err(|_| "Không lưu được lịch sử".to_string())?;
    let _ = app.emit("history-changed", ());
    Ok(result)
}

#[tauri::command]
pub fn history_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
) -> Result<Vec<crate::db::Conversation>, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không xóa được lịch sử")?;
    let mut db = history_db(&app)?;
    let result = crate::db::delete_history(&mut db, &id)
        .map_err(|_| "Không xóa được lịch sử".to_string())?;
    let _ = app.emit("history-changed", ());
    Ok(result)
}

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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không đọc được ghi chú")?;
    let db = local_db(&app)?;
    crate::db::notes::list(&db, &query).map_err(|_| "Không tìm được ghi chú".to_string())
}
#[tauri::command]
pub fn notes_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    input: crate::db::notes::NoteInput,
) -> Result<crate::db::notes::Note, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không lưu được ghi chú")?;
    let mut db = local_db(&app)?;
    let note = crate::db::notes::save(&mut db, input).map_err(note_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không xóa được ghi chú")?;
    let mut db = local_db(&app)?;
    crate::db::notes::delete(&mut db, &id, rev).map_err(note_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không đọc được thói quen")?;
    let db = local_db(&app)?;
    crate::db::habits::list(&db, &today).map_err(habit_error)
}
#[tauri::command]
pub fn habits_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    input: crate::db::habits::HabitInput,
) -> Result<crate::db::habits::Habit, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không lưu được thói quen")?;
    let mut db = local_db(&app)?;
    let habit = crate::db::habits::save(&mut db, input).map_err(habit_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không xóa được thói quen")?;
    let mut db = local_db(&app)?;
    crate::db::habits::delete(&mut db, &id, rev).map_err(habit_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không lưu được thói quen")?;
    let mut db = local_db(&app)?;
    crate::db::habits::check_in(&mut db, &id, &date, done).map_err(habit_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không đọc được lịch")?;
    let db = local_db(&app)?;
    crate::db::events::range(&db, from, until, tz_offset).map_err(event_error)
}
#[tauri::command]
pub fn events_save(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    input: crate::db::events::EventInput,
) -> Result<crate::db::events::Event, String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không lưu được sự kiện")?;
    let mut db = local_db(&app)?;
    let event = crate::db::events::save(&mut db, input).map_err(event_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không xóa được sự kiện")?;
    let mut db = local_db(&app)?;
    crate::db::events::delete(&mut db, &id, rev).map_err(event_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không lưu được sự kiện")?;
    let mut db = local_db(&app)?;
    let event = crate::db::events::set_done(&mut db, &id, rev, done).map_err(event_error)?;
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
    let _guard = state.history_lock.lock().map_err(|_| "Không lưu được múi giờ")?;
    let db = local_db(&app)?;
    db.execute(
        "INSERT INTO sync_state (key, value) VALUES ('tz_offset', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [offset.to_string()],
    )
    .map_err(|_| "Không lưu được múi giờ".to_string())?;
    Ok(())
}
#[tauri::command]
pub fn reminders_pending(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<Vec<crate::db::reminders::Pending>, String> {
    let _guard = state.history_lock.lock().map_err(|_| "Không đọc được nhắc nhở")?;
    let db = local_db(&app)?;
    crate::db::reminders::pending(&db).map_err(reminder_error)
}
#[tauri::command]
pub fn reminders_dismiss(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    id: String,
) -> Result<(), String> {
    let _guard = state.history_lock.lock().map_err(|_| "Không cập nhật được nhắc nhở")?;
    let mut db = local_db(&app)?;
    crate::db::reminders::dismiss(&mut db, &id).map_err(reminder_error)?;
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
    let _guard = state.history_lock.lock().map_err(|_| "Không cập nhật được nhắc nhở")?;
    let mut db = local_db(&app)?;
    crate::db::reminders::snooze(&mut db, &id, minutes).map_err(reminder_error)?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không đọc được lịch sử hoạt động")?;
    let db = local_db(&app)?;
    crate::db::activity::list(&db, kind.as_deref(), from, until)
        .map_err(|_| "Không đọc được lịch sử hoạt động".into())
}
#[tauri::command]
pub fn activity_delete(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    ids: Vec<String>,
) -> Result<(), String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không xóa được lịch sử hoạt động")?;
    let mut db = local_db(&app)?;
    crate::db::activity::delete(&mut db, &ids).map_err(|_| "Không xóa được lịch sử hoạt động")?;
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
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "Không xóa được lịch sử hoạt động")?;
    let mut db = local_db(&app)?;
    let count = crate::db::activity::delete_range(&mut db, from, until)
        .map_err(|_| "Không xóa được lịch sử hoạt động".to_string())?;
    let _ = app.emit("activity-changed", ());
    Ok(count)
}
