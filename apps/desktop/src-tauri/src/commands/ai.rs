use super::{record_activity, Runtime};
use crate::{config, secrets};
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
        "openai": secrets::get("openai").is_ok(),
        "cloud": secrets::get("cloud").is_ok(),
        "cloudflare_ai": secrets::get("cloudflare_ai").is_ok(),
    })
}

/// Keychain entry holding the bearer token for a Cloudflare-backed model.
fn backend_secret(backend: &crate::ai::Backend) -> &'static str {
    match backend {
        crate::ai::Backend::WorkersAi { .. } => "cloudflare_ai",
        _ => "cloud",
    }
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
            || messages
                .as_array()
                .is_some_and(|a| a.is_empty() || a.len() > 1000)
            || messages.to_string().len() > 1_000_000
        {
            return Err("Hội thoại quá dài. Hãy tạo cuộc trò chuyện mới.".into());
        }
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
                let _ = app.emit_to(
                    window.label(),
                    "chat-delta",
                    json!({"id": id, "text": text}),
                );
                let _ = app.emit("mascot-state", "talking");
            };
            let result = match &backend {
                crate::ai::Backend::Claude { model } => {
                    let key = secrets::get("claude")?;
                    crate::ai::stream_anthropic(
                        &state.client,
                        &crate::ai::anthropic_api(),
                        &key,
                        model,
                        &system,
                        &messages,
                        max_tokens,
                        cancel.clone(),
                        |text| on_delta(text),
                    )
                    .await
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
                crate::ai::Backend::Cloudflare { base, model }
                | crate::ai::Backend::WorkersAi { base, model } => {
                    let key = secrets::get(backend_secret(&backend)).ok();
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
            crate::ai::Backend::Claude { model } => {
                let key = secrets::get("claude")?;
                crate::ai::stream_anthropic(
                    &state.client,
                    &crate::ai::anthropic_api(),
                    &key,
                    model,
                    system,
                    &messages,
                    max_tokens,
                    cancel,
                    |text| out.push_str(text),
                )
                .await
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
                    system,
                    &messages,
                    max_tokens,
                    cancel,
                    |text| out.push_str(text),
                )
                .await
            }
            crate::ai::Backend::Cloudflare { base, model }
            | crate::ai::Backend::WorkersAi { base, model } => {
                let key = secrets::get(backend_secret(&backend)).ok();
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

#[tauri::command]
pub async fn ai_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    target: String,
) -> Result<Vec<String>, String> {
    let c = config::read(&app)?;
    let (base, key) = match target.as_str() {
        "local" => (
            crate::ai::ollama::normalize_local_base(&config::string(
                &c,
                "localBase",
                crate::ai::DEFAULT_LOCAL_BASE,
            )),
            None,
        ),
        "external" => (
            config::string(&c, "openaiBase", "https://api.openai.com/v1"),
            secrets::get("openai").ok(),
        ),
        "workersai" => {
            let account = config::string(&c, "workersAiAccount", "");
            if !crate::ai::valid_account_id(&account) {
                return Err("Chưa cấu hình tài khoản Cloudflare hợp lệ".into());
            }
            let key = secrets::get("cloudflare_ai").ok();
            let base = crate::ai::workers_ai_base(&account);
            return crate::ai::list_models(&state.client, &base, key.as_deref()).await;
        }
        _ => return Err("Nguồn model không hợp lệ".into()),
    };
    crate::ai::list_models(&state.client, &base, key.as_deref()).await
}

#[tauri::command]
pub async fn local_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<crate::ai::ollama::LocalServerStatus, String> {
    let c = config::read(&app)?;
    let base = config::string(&c, "localBase", crate::ai::DEFAULT_LOCAL_BASE);
    crate::ai::ollama::status(&state.client, &base, &state.local_pull).await
}

#[tauri::command]
pub async fn local_pull_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    model: String,
) -> Result<crate::ai::ollama::LocalPullProgress, String> {
    let c = config::read(&app)?;
    let base = config::string(&c, "localBase", crate::ai::DEFAULT_LOCAL_BASE);
    crate::ai::ollama::pull_model(
        &state.client,
        &base,
        &model,
        state.local_pull.clone(),
        |progress| {
            let _ = app.emit("local-pull-progress", progress);
        },
    )
    .await
}

#[tauri::command]
pub fn local_cancel_pull(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Option<crate::ai::ollama::LocalPullProgress> {
    let cancelled = state.local_pull.cancel();
    if let Some(ref p) = cancelled {
        let _ = app.emit("local-pull-progress", p);
    }
    cancelled
}

#[tauri::command]
pub async fn local_delete_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    model: String,
) -> Result<(), String> {
    let c = config::read(&app)?;
    let base = config::string(&c, "localBase", crate::ai::DEFAULT_LOCAL_BASE);
    crate::ai::ollama::delete_model(&state.client, &base, &model).await
}

#[tauri::command]
pub async fn test_connection(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    name: String,
) -> Result<String, String> {
    let c = config::read(&app)?;
    match name.as_str() {
        "claude" => {
            let key = secrets::get("claude")?;
            let model = config::string(&c, "model", "claude-haiku-5-5");
            let mut reply = String::new();
            crate::ai::stream_anthropic(
                &state.client,
                &crate::ai::anthropic_api(),
                &key,
                &model,
                "Reply only OK.",
                &json!([{"role": "user", "content": "Reply OK"}]),
                16,
                Arc::new(tokio::sync::Notify::new()),
                |t| reply.push_str(t),
            )
            .await?;
            Ok("Kết nối Claude API thành công".into())
        }
        "openai" => {
            let base = config::string(&c, "openaiBase", "https://api.openai.com/v1");
            let key = secrets::get("openai").ok();
            let list = crate::ai::list_models(&state.client, &base, key.as_deref()).await?;
            Ok(format!("Kết nối thành công ({} model)", list.len()))
        }
        "local" => {
            let base = config::string(&c, "localBase", crate::ai::DEFAULT_LOCAL_BASE);
            let st = crate::ai::ollama::status(&state.client, &base, &state.local_pull).await?;
            if st.online {
                Ok(format!(
                    "Máy chủ Ollama đang chạy ({} model đã cài)",
                    st.installed_models.len()
                ))
            } else {
                Err("Không kết nối được Ollama cục bộ".into())
            }
        }
        _ => Err(format!("Không hỗ trợ kiểm tra kết nối cho {name}")),
    }
}
