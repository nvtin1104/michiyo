use super::{record_activity, Runtime};
use crate::{
    config,
    providers::{Provider, Translation, Translator},
    secrets,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub fn save_secret(app: tauri::AppHandle, name: String, value: String) -> Result<(), String> {
    secrets::set(&name, &value)?;
    let _ = app.emit("secrets-changed", ());
    Ok(())
}

#[tauri::command]
pub fn secret_status() -> Value {
    json!({
        "claude": secrets::get("claude").is_ok(),
        "google": secrets::get("google").is_ok(),
        "libre": secrets::get("libre").is_ok(),
        "openai": secrets::get("openai").is_ok(),
        "cloud": secrets::get("cloud").is_ok(),
    })
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
        let _ = app.emit("mascot-state", "thinking");
        if !messages.is_array()
            || messages.as_array().is_some_and(|a| a.is_empty() || a.len() > 1000)
            || messages.to_string().len() > 1_000_000
        {
            return Err("Hội thoại quá dài. Hãy tạo cuộc trò chuyện mới.".into());
        }
        let request = json!({
            "action": "chat",
            "model": config::string(&c, "model", "claude-haiku-5-5"),
            "system": config::string(&c, "systemPrompt", "You are Michiyo, a friendly conversational companion."),
            "maxTokens": c["maxTokens"].as_u64().unwrap_or(2048).clamp(1, 16384),
            "messages": messages,
        });
        let backends = crate::ai::plan(&c)?;
        let system = config::string(
            &c,
            "systemPrompt",
            "You are Michiyo, a friendly conversational companion.",
        );
        let max_tokens = c["maxTokens"].as_u64().unwrap_or(2048).clamp(1, 16384);
        let mut last_error = String::new();
        let mut sent_any = false;
        for backend in backends {
            let mut on_delta = |text: &str| {
                sent_any = true;
                let _ = app.emit_to(window.label(), "chat-delta", json!({"id": id, "text": text}));
                let _ = app.emit("mascot-state", "talking");
            };
            let result = match &backend {
                crate::ai::Backend::Claude { .. } => {
                    crate::providers::agent::run(
                        &app,
                        request.clone(),
                        cancel.clone(),
                        |text| on_delta(text),
                    )
                    .await
                    .map(|_| ())
                }
                crate::ai::Backend::OpenAi {
                    base,
                    model,
                    use_key,
                } => {
                    let key = if *use_key {
                        secrets::get("openai").ok()
                    } else {
                        None
                    };
                    crate::ai::stream_chat(
                        &state.client,
                        base,
                        key.as_deref(),
                        model,
                        &system,
                        &messages,
                        max_tokens,
                        cancel.clone(),
                        |text| on_delta(text),
                    )
                    .await
                }
                crate::ai::Backend::Cloudflare { base, model } => {
                    let key = secrets::get("cloud").ok();
                    crate::ai::stream_chat(
                        &state.client,
                        base,
                        key.as_deref(),
                        model,
                        &system,
                        &messages,
                        max_tokens,
                        cancel.clone(),
                        |text| on_delta(text),
                    )
                    .await
                }
            };
            match result {
                Ok(()) => return Ok(()),
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
            crate::ai::Backend::OpenAi {
                base,
                model,
                use_key,
            } => {
                let key = if *use_key {
                    secrets::get("openai").ok()
                } else {
                    None
                };
                crate::ai::stream_chat(
                    &state.client,
                    base,
                    key.as_deref(),
                    model,
                    system,
                    &messages,
                    max_tokens,
                    cancel,
                    |text| out.push_str(text),
                )
                .await
            }
            crate::ai::Backend::Cloudflare { base, model } => {
                let key = secrets::get("cloud").ok();
                crate::ai::stream_chat(
                    &state.client,
                    base,
                    key.as_deref(),
                    model,
                    system,
                    &messages,
                    max_tokens,
                    cancel,
                    |text| out.push_str(text),
                )
                .await
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
    let raw = complete_once(
        &app,
        &state,
        &crate::ai::actions::suggest_system(&now_local),
        &text,
        800,
    )
    .await?;
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
    let token = body["token"]
        .as_str()
        .filter(|t| t.starts_with("md_"))
        .ok_or("Phản hồi từ Michiyo Cloud không hợp lệ")?;
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
        "local" => (
            config::string(&c, "localBase", "http://127.0.0.1:11434/v1"),
            None,
        ),
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
        crate::providers::agent::run(
            &app,
            json!({
                "action": "check",
                "model": config::string(&c, "model", "claude-haiku-5-5"),
                "system": "Reply only OK.",
                "maxTokens": 32,
                "messages": [{"role": "user", "content": "Reply OK"}],
            }),
            Arc::new(tokio::sync::Notify::new()),
            |_| {},
        )
        .await?;
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
