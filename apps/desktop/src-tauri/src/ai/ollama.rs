use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalInstalledModel {
    pub name: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalRecommendedModel {
    pub id: String,
    pub label: String,
    pub size_hint: String,
    pub ram_hint: String,
    pub description: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalPullProgress {
    pub model: String,
    pub status: String,
    pub phase: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub percent: u8,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalServerStatus {
    pub online: bool,
    pub base_url: String,
    pub version: Option<String>,
    pub message: Option<String>,
    pub installed_models: Vec<LocalInstalledModel>,
    pub active_pull: Option<LocalPullProgress>,
    pub recommended_models: Vec<LocalRecommendedModel>,
}

#[derive(Default)]
pub struct PullTracker {
    progress: Mutex<Option<LocalPullProgress>>,
    cancel: Mutex<Option<Arc<Notify>>>,
}

impl PullTracker {
    pub fn snapshot(&self) -> Option<LocalPullProgress> {
        self.progress.lock().unwrap().clone()
    }

    pub fn begin(&self, model: &str) -> Result<(Arc<Notify>, LocalPullProgress), String> {
        let mut slot = self.progress.lock().unwrap();
        if slot.as_ref().is_some_and(|p| p.status == "pulling") {
            return Err("Đang tải một model khác. Hãy đợi hoàn tất hoặc bấm Huỷ tải.".into());
        }
        let notify = Arc::new(Notify::new());
        let initial = LocalPullProgress {
            model: model.to_string(),
            status: "pulling".into(),
            phase: "Đang kết nối máy chủ Ollama…".into(),
            completed_bytes: 0,
            total_bytes: 0,
            percent: 0,
            error: None,
        };
        *slot = Some(initial.clone());
        *self.cancel.lock().unwrap() = Some(notify.clone());
        Ok((notify, initial))
    }

    pub fn update(&self, next: LocalPullProgress) {
        if next.status != "pulling" {
            *self.cancel.lock().unwrap() = None;
        }
        *self.progress.lock().unwrap() = Some(next);
    }

    pub fn cancel(&self) -> Option<LocalPullProgress> {
        if let Some(notify) = self.cancel.lock().unwrap().take() {
            notify.notify_waiters();
        }
        let mut slot = self.progress.lock().unwrap();
        if let Some(current) = slot.as_mut() {
            if current.status == "pulling" {
                current.status = "cancelled".into();
                current.phase = "Đã huỷ tải model".into();
            }
            return Some(current.clone());
        }
        None
    }
}

pub fn recommended_models() -> Vec<LocalRecommendedModel> {
    vec![
        LocalRecommendedModel {
            id: "gemma3:4b".into(),
            label: "Google Gemma 3 · 4B".into(),
            size_hint: "~3,3 GB".into(),
            ram_hint: "RAM ≥ 8 GB".into(),
            description: "Khuyên dùng cho chat, tóm tắt và dịch tiếng Việt trên máy cá nhân."
                .into(),
            recommended: true,
        },
        LocalRecommendedModel {
            id: "gemma3:1b".into(),
            label: "Google Gemma 3 · 1B".into(),
            size_hint: "~815 MB".into(),
            ram_hint: "RAM ≥ 4 GB".into(),
            description: "Siêu nhẹ, phản hồi tức thì trên máy cấu hình thấp.".into(),
            recommended: false,
        },
        LocalRecommendedModel {
            id: "gemma3:12b".into(),
            label: "Google Gemma 3 · 12B".into(),
            size_hint: "~8,1 GB".into(),
            ram_hint: "RAM ≥ 16 GB".into(),
            description: "Chất lượng suy luận và dịch thuật cao cho máy có GPU/RAM lớn.".into(),
            recommended: false,
        },
        LocalRecommendedModel {
            id: "gemma2:9b".into(),
            label: "Google Gemma 2 · 9B".into(),
            size_hint: "~5,4 GB".into(),
            ram_hint: "RAM ≥ 12 GB".into(),
            description: "Bản ổn định của Google, cân bằng tốc độ và độ chính xác.".into(),
            recommended: false,
        },
    ]
}

/// Normalizes a local base URL so OpenAI-compatible endpoints always have `/v1` when the user enters a root Ollama URL.
pub fn normalize_local_base(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return super::DEFAULT_LOCAL_BASE.into();
    }
    if let Ok(parsed) = reqwest::Url::parse(trimmed) {
        if parsed.path().is_empty() || parsed.path() == "/" {
            return format!("{trimmed}/v1");
        }
    }
    trimmed.to_string()
}

/// Extracts the Ollama native root URL (strips trailing `/v1`) and validates localhost/HTTPS safety.
pub fn ollama_root_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let base = if trimmed.is_empty() {
        "http://127.0.0.1:11434"
    } else {
        trimmed.strip_suffix("/v1").unwrap_or(trimmed)
    };
    super::validate_base(base)?;
    Ok(base.to_string())
}

pub fn validate_model_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 120 {
        return Err("Tên model không hợp lệ".into());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-' | '/'))
    {
        return Err("Tên model chỉ được chứa chữ, số và ký tự . _ : - /".into());
    }
    Ok(trimmed.to_string())
}

fn translate_phase(raw: &str) -> String {
    let s = raw.trim();
    if s == "pulling manifest" {
        "Đang đọc thông tin model…".into()
    } else if s.starts_with("pulling ") {
        "Đang tải dữ liệu model…".into()
    } else if s.contains("verifying") {
        "Đang kiểm tra tệp…".into()
    } else if s.contains("writing manifest") {
        "Đang ghi cấu hình…".into()
    } else if s.contains("removing") {
        "Đang hoàn tất…".into()
    } else if s == "success" {
        "Đã cài xong".into()
    } else if s.is_empty() {
        "Đang tải…".into()
    } else {
        s.to_string()
    }
}

pub async fn status(
    client: &reqwest::Client,
    raw_base: &str,
    tracker: &PullTracker,
) -> Result<LocalServerStatus, String> {
    let root = ollama_root_url(raw_base)?;
    let version_req = client
        .get(format!("{root}/api/version"))
        .timeout(std::time::Duration::from_secs(4))
        .send();
    let tags_req = client
        .get(format!("{root}/api/tags"))
        .timeout(std::time::Duration::from_secs(5))
        .send();
    let (version_res, tags_res) = tokio::join!(version_req, tags_req);

    let version = match version_res {
        Ok(resp) if resp.status().is_success() => resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v["version"].as_str().map(str::to_owned)),
        _ => None,
    };

    if let Ok(resp) = tags_res {
        if resp.status().is_success() {
            let body: Value = resp
                .json()
                .await
                .map_err(|_| "Phản hồi từ Ollama không hợp lệ".to_string())?;
            let mut installed_models = Vec::new();
            if let Some(items) = body["models"].as_array() {
                for item in items {
                    let Some(name) = item["name"].as_str().or_else(|| item["model"].as_str())
                    else {
                        continue;
                    };
                    installed_models.push(LocalInstalledModel {
                        name: name.to_string(),
                        size_bytes: item["size"].as_u64().unwrap_or(0),
                        modified_at: item["modified_at"].as_str().unwrap_or_default().to_string(),
                        family: item["details"]["family"].as_str().map(str::to_owned),
                        parameter_size: item["details"]["parameter_size"]
                            .as_str()
                            .map(str::to_owned),
                        quantization: item["details"]["quantization_level"]
                            .as_str()
                            .map(str::to_owned),
                    });
                }
            }
            return Ok(LocalServerStatus {
                online: true,
                base_url: root,
                version,
                message: None,
                installed_models,
                active_pull: tracker.snapshot(),
                recommended_models: recommended_models(),
            });
        }
    }

    // Fallback for OpenAI-compatible local servers (e.g. LM Studio) that only expose `/v1/models`.
    let v1 = normalize_local_base(raw_base);
    if let Ok(ids) = super::list_models(client, &v1, None).await {
        let installed_models = ids
            .into_iter()
            .map(|name| LocalInstalledModel {
                name,
                size_bytes: 0,
                modified_at: String::new(),
                family: None,
                parameter_size: None,
                quantization: None,
            })
            .collect();
        return Ok(LocalServerStatus {
            online: true,
            base_url: root,
            version,
            message: None,
            installed_models,
            active_pull: tracker.snapshot(),
            recommended_models: recommended_models(),
        });
    }

    Ok(LocalServerStatus {
        online: false,
        base_url: root,
        version: None,
        message: Some(
            "Chưa kết nối được Ollama trên máy. Hãy cài Ollama hoặc chạy lệnh ollama serve.".into(),
        ),
        installed_models: Vec::new(),
        active_pull: tracker.snapshot(),
        recommended_models: recommended_models(),
    })
}

pub async fn pull_model(
    client: &reqwest::Client,
    raw_base: &str,
    model: &str,
    tracker: Arc<PullTracker>,
    mut on_progress: impl FnMut(&LocalPullProgress),
) -> Result<LocalPullProgress, String> {
    let root = ollama_root_url(raw_base)?;
    let model = validate_model_name(model)?;
    let (cancel, initial) = tracker.begin(&model)?;
    on_progress(&initial);

    let request = client
        .post(format!("{root}/api/pull"))
        .timeout(std::time::Duration::from_secs(3600))
        .json(&json!({"model": model, "stream": true}));

    let mut response = tokio::select! {
        _ = cancel.notified() => {
            let cancelled = tracker.cancel().unwrap_or(initial);
            on_progress(&cancelled);
            return Ok(cancelled);
        }
        sent = request.send() => match sent {
            Ok(resp) => resp,
            Err(_) => {
                let err_msg = "Không kết nối được Ollama. Hãy chắc chắn dịch vụ Ollama đang chạy.".to_string();
                let failed = LocalPullProgress {
                    model,
                    status: "failed".into(),
                    phase: err_msg.clone(),
                    completed_bytes: 0,
                    total_bytes: 0,
                    percent: 0,
                    error: Some(err_msg.clone()),
                };
                tracker.update(failed.clone());
                on_progress(&failed);
                return Err(err_msg);
            }
        }
    };

    if !response.status().is_success() {
        let err_msg = format!(
            "Ollama trả về HTTP {} khi tải model.",
            response.status().as_u16()
        );
        let failed = LocalPullProgress {
            model,
            status: "failed".into(),
            phase: err_msg.clone(),
            completed_bytes: 0,
            total_bytes: 0,
            percent: 0,
            error: Some(err_msg.clone()),
        };
        tracker.update(failed.clone());
        on_progress(&failed);
        return Err(err_msg);
    }

    let mut buffer = Vec::<u8>::new();
    let mut current = tracker.snapshot().unwrap_or(initial);

    loop {
        let chunk = tokio::select! {
            _ = cancel.notified() => {
                let cancelled = tracker.cancel().unwrap_or(current);
                on_progress(&cancelled);
                return Ok(cancelled);
            }
            next = response.chunk() => match next {
                Ok(c) => c,
                Err(_) => {
                    let err_msg = "Kết nối tải model bị ngắt giữa chừng.".to_string();
                    current.status = "failed".into();
                    current.phase = err_msg.clone();
                    current.error = Some(err_msg.clone());
                    tracker.update(current.clone());
                    on_progress(&current);
                    return Err(err_msg);
                }
            }
        };

        let Some(bytes) = chunk else {
            break;
        };
        buffer.extend_from_slice(&bytes);

        while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if let Some(err) = val["error"].as_str() {
                let err_msg = format!("Không tải được model: {err}");
                current.status = "failed".into();
                current.phase = err_msg.clone();
                current.error = Some(err_msg.clone());
                tracker.update(current.clone());
                on_progress(&current);
                return Err(err_msg);
            }
            if let Some(raw_status) = val["status"].as_str() {
                current.phase = translate_phase(raw_status);
                if raw_status == "success" {
                    current.status = "completed".into();
                    current.percent = 100;
                    if current.total_bytes > 0 {
                        current.completed_bytes = current.total_bytes;
                    }
                }
            }
            if let Some(total) = val["total"].as_u64() {
                if total > 0 {
                    current.total_bytes = total;
                }
            }
            if let Some(completed) = val["completed"].as_u64() {
                current.completed_bytes = completed;
                if current.total_bytes > 0 {
                    let pct = ((completed as f64 / current.total_bytes as f64) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8;
                    current.percent = pct;
                }
            }
            tracker.update(current.clone());
            on_progress(&current);
        }
    }

    if current.status == "pulling" {
        current.status = "completed".into();
        current.phase = "Đã cài xong".into();
        current.percent = 100;
        tracker.update(current.clone());
        on_progress(&current);
    }

    Ok(current)
}

pub async fn delete_model(
    client: &reqwest::Client,
    raw_base: &str,
    model: &str,
) -> Result<(), String> {
    let root = ollama_root_url(raw_base)?;
    let model = validate_model_name(model)?;
    let response = client
        .delete(format!("{root}/api/delete"))
        .timeout(std::time::Duration::from_secs(15))
        .json(&json!({"model": model}))
        .send()
        .await
        .map_err(|_| "Không kết nối được Ollama để xoá model.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Không xoá được model (HTTP {}).",
            response.status().as_u16()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn normalizes_and_validates_ollama_urls() {
        assert_eq!(
            normalize_local_base("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            normalize_local_base("http://127.0.0.1:11434/v1/"),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            ollama_root_url("http://127.0.0.1:11434/v1").unwrap(),
            "http://127.0.0.1:11434"
        );
        assert!(ollama_root_url("http://example.com:11434").is_err());
        assert_eq!(validate_model_name("gemma3:4b").unwrap(), "gemma3:4b");
        assert!(validate_model_name("bad model; rm -rf").is_err());
    }

    #[tokio::test]
    async fn fetches_ollama_status_and_installed_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "0.6.2"})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [{
                    "name": "gemma3:4b",
                    "size": 3300000000u64,
                    "modified_at": "2026-10-30T10:00:00Z",
                    "details": {
                        "family": "gemma3",
                        "parameter_size": "4.3B",
                        "quantization_level": "Q4_K_M"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let tracker = PullTracker::default();
        let st = status(
            &reqwest::Client::new(),
            &format!("{}/v1", server.uri()),
            &tracker,
        )
        .await
        .unwrap();
        assert!(st.online);
        assert_eq!(st.version.as_deref(), Some("0.6.2"));
        assert_eq!(st.installed_models.len(), 1);
        assert_eq!(st.installed_models[0].name, "gemma3:4b");
        assert_eq!(
            st.installed_models[0].parameter_size.as_deref(),
            Some("4.3B")
        );
    }

    #[tokio::test]
    async fn streams_pull_progress_and_deletes_model() {
        let server = MockServer::start().await;
        let ndjson = concat!(
            "{\"status\":\"pulling manifest\"}\n",
            "{\"status\":\"pulling abcdef\",\"completed\":500,\"total\":1000}\n",
            "{\"status\":\"verifying sha256 digest\"}\n",
            "{\"status\":\"success\"}\n"
        );
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(ndjson, "application/x-ndjson"))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/delete"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let tracker = Arc::new(PullTracker::default());
        let mut phases = Vec::new();
        let final_state = pull_model(
            &reqwest::Client::new(),
            &server.uri(),
            "gemma3:4b",
            tracker.clone(),
            |p| phases.push((p.percent, p.status.clone())),
        )
        .await
        .unwrap();
        assert_eq!(final_state.status, "completed");
        assert_eq!(final_state.percent, 100);
        assert!(phases.iter().any(|(pct, _)| *pct == 50));

        delete_model(&reqwest::Client::new(), &server.uri(), "gemma3:4b")
            .await
            .unwrap();
    }
}
