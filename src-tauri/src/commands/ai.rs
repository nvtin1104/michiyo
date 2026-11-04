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
        "cloudflare_deploy": secrets::get("cloudflare_deploy").is_ok(),
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
    _state: tauri::State<'_, Runtime>,
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
        Ok("Kết nối thành công".into())
    } else {
        Err(format!("Không hỗ trợ kiểm tra kết nối cho {name}"))
    }
}

#[tauri::command]
pub async fn cloud_backend_verify(
    _app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    token: String,
) -> Result<Vec<crate::cloud_setup::cloudflare::Account>, String> {
    let cf = crate::cloud_setup::cloudflare::Cf::new(&state.client, &crate::ai::cloudflare_api(), &token);
    cf.accounts().await
}

#[tauri::command]
pub async fn cloud_ai_verify(
    _app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    token: String,
) -> Result<Vec<crate::cloud_setup::cloudflare::Account>, String> {
    let cf = crate::cloud_setup::cloudflare::Cf::new(&state.client, &crate::ai::cloudflare_api(), &token);
    let accounts = cf.accounts().await?;
    if let Some(acc) = accounts.first() {
        let _ = cf.text_models(&acc.id).await?;
    }
    Ok(accounts)
}

#[tauri::command]
pub async fn cloud_backend_deploy(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    account_id: String,
    token: String,
    remember: bool,
) -> Result<Value, String> {
    let manifest: crate::cloud_setup::Manifest = serde_json::from_str(crate::cloud_setup::MANIFEST_JSON)
        .map_err(|e| format!("Lỗi đọc manifest backend: {}", e))?;

    let emit_progress = |progress: crate::cloud_setup::DeployProgress| {
        let _ = app.emit("cloud-deploy-progress", progress);
    };

    let cf = crate::cloud_setup::cloudflare::Cf::new(&state.client, &crate::ai::cloudflare_api(), &token);

    emit_progress(crate::cloud_setup::DeployProgress {
        step: "d1".into(),
        status: "running".into(),
        message: "Đang kiểm tra và khởi tạo cơ sở dữ liệu D1...".into(),
    });

    let d1_id = cf.ensure_d1(&account_id, "michiyo").await?;

    crate::cloud_setup::run_migrations(&cf, &account_id, &d1_id, &manifest, &emit_progress).await?;

    let (worker_url, bootstrap_secret) =
        crate::cloud_setup::deploy_worker(&cf, &account_id, &d1_id, &manifest, &emit_progress).await?;

    emit_progress(crate::cloud_setup::DeployProgress {
        step: "pairing".into(),
        status: "running".into(),
        message: "Đang tự động ghép nối với máy tính này làm thiết bị quản trị...".into(),
    });

    let (device_id, device_token) =
        crate::cloud_setup::bootstrap_pairing(&state.client, &worker_url, &bootstrap_secret, "Michiyo Desktop").await?;

    secrets::set("cloud", &device_token)?;

    if remember {
        let _ = secrets::set("cloudflare_deploy", &token);
    } else {
        let _ = secrets::set("cloudflare_deploy", "");
    }

    emit_progress(crate::cloud_setup::DeployProgress {
        step: "completed".into(),
        status: "done".into(),
        message: "Hoàn tất triển khai backend cá nhân!".into(),
    });

    let _ = app.emit("secrets-changed", ());

    Ok(json!({
        "url": worker_url,
        "device_id": device_id,
        "d1_id": d1_id,
        "version": manifest.version
    }))
}

#[tauri::command]
pub async fn cloud_add_device(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
    name: String,
) -> Result<Value, String> {
    let c = config::read(&app)?;
    let cloud_url = config::string(&c, "cloudUrl", "").trim_end_matches('/').to_string();
    if cloud_url.is_empty() {
        return Err("Chưa cấu hình địa chỉ Michiyo Cloud".into());
    }
    let token = secrets::get("cloud").map_err(|_| "Chưa ghép nối thiết bị quản trị".to_string())?;

    let res = state
        .client
        .post(format!("{cloud_url}/v1/pair/start"))
        .bearer_auth(token)
        .json(&json!({ "name": name }))
        .send()
        .await
        .map_err(|e| format!("Không kết nối được Cloud backend: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Cloud backend báo lỗi HTTP {}", res.status()));
    }
    res.json().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cloud_open_admin(
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<String, String> {
    let c = config::read(&app)?;
    let cloud_url = config::string(&c, "cloudUrl", "").trim_end_matches('/').to_string();
    if cloud_url.is_empty() {
        return Err("Chưa cấu hình địa chỉ Michiyo Cloud".into());
    }
    let token = secrets::get("cloud").map_err(|_| "Chưa ghép nối thiết bị quản trị".to_string())?;

    let res = state
        .client
        .post(format!("{cloud_url}/v1/admin/web-login"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Không kết nối được Cloud backend: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Cloud backend báo lỗi HTTP {}", res.status()));
    }

    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    let code = body["code"].as_str().ok_or("Thiếu mã đăng nhập web")?;
    let admin_url = format!("{}/login#code={}", cloud_url, code);

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &admin_url])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&admin_url)
            .spawn();
    }
    Ok(admin_url)
}
