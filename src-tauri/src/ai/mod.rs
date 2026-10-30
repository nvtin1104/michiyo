//! Chooses which model answers a chat and enforces the Offline/Online/Hybrid mode.
pub mod actions;
pub mod ollama;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Notify;

pub const DEFAULT_LOCAL_BASE: &str = "http://127.0.0.1:11434/v1";
const NOT_CONNECTED: &str = "Chưa kết nối cloud. Model Cloudflare cần backend Michiyo Cloud.";

#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    Claude {
        model: String,
    },
    /// Any server that speaks `POST {base}/chat/completions` (OpenAI, OpenRouter, Ollama, LM Studio…).
    OpenAi {
        base: String,
        model: String,
        use_key: bool,
    },
    /// Michiyo Cloud Worker: OpenAI-compatible at `{base}/chat/completions`, authenticated by the device token.
    Cloudflare {
        base: String,
        model: String,
    },
}

impl Backend {
    pub fn is_local(&self) -> bool {
        matches!(self, Backend::OpenAi { base, .. } | Backend::Cloudflare { base, .. } if is_local_url(base))
    }
}

fn text<'a>(config: &'a Value, key: &str, default: &'a str) -> &'a str {
    config[key]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(default)
}

pub fn mode(config: &Value) -> &str {
    match config["mode"].as_str() {
        Some("offline") => "offline",
        Some("hybrid") => "hybrid",
        _ => "online",
    }
}

pub fn is_local_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    })
}

/// HTTPS anywhere, plain HTTP only for this machine.
pub fn validate_base(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Địa chỉ model không hợp lệ")?;
    if parsed.scheme() == "https" || (parsed.scheme() == "http" && is_local_url(url)) {
        Ok(())
    } else {
        Err("Địa chỉ model cần HTTPS hoặc localhost".into())
    }
}

/// Backends to try in order. Offline never leaves the machine; Hybrid falls back to the local model.
pub fn plan(config: &Value) -> Result<Vec<Backend>, String> {
    let local_base = ollama::normalize_local_base(text(config, "localBase", DEFAULT_LOCAL_BASE));
    let local_model = text(config, "localModel", "");
    let local = (!local_model.is_empty()).then(|| Backend::OpenAi {
        base: local_base.clone(),
        model: local_model.into(),
        use_key: false,
    });
    let primary = if config["modelSource"].as_str() == Some("cloudflare") {
        let url = text(config, "cloudUrl", "").trim_end_matches('/');
        if url.is_empty() {
            return Err(NOT_CONNECTED.into());
        }
        Backend::Cloudflare {
            base: format!("{url}/v1/ai"),
            model: text(config, "cloudModel", "").into(),
        }
    } else if config["chatProvider"].as_str() == Some("local") {
        if local_model.is_empty() {
            return Err(
                "Chưa chọn model cục bộ. Hãy tải hoặc chọn model trong Cài đặt → AI.".into(),
            );
        }
        Backend::OpenAi {
            base: local_base,
            model: local_model.into(),
            use_key: false,
        }
    } else if config["chatProvider"].as_str() == Some("openai_compat") {
        Backend::OpenAi {
            base: text(config, "openaiBase", "https://api.openai.com/v1")
                .trim_end_matches('/')
                .into(),
            model: text(config, "openaiModel", "").into(),
            use_key: true,
        }
    } else {
        Backend::Claude {
            model: text(config, "model", "claude-haiku-5-5").into(),
        }
    };
    let backends = match mode(config) {
        "offline" => vec![if primary.is_local() {
            primary
        } else {
            local.ok_or("Chế độ Offline cần model chạy trên máy này (ví dụ Ollama). Hãy cấu hình trong Cài đặt → AI.")?
        }],
        "hybrid" => {
            let mut list = vec![primary];
            if let Some(local_backend) = local {
                if list[0] != local_backend {
                    list.push(local_backend);
                }
            }
            list
        }
        _ => vec![primary],
    };
    for backend in &backends {
        match backend {
            Backend::OpenAi { base, model, .. } => {
                validate_base(base)?;
                if model.is_empty() {
                    return Err("Chưa chọn model. Hãy chọn model trong Cài đặt → AI.".into());
                }
            }
            Backend::Cloudflare { base, .. } => validate_base(base)?,
            Backend::Claude { .. } => {}
        }
    }
    Ok(backends)
}

/// Offline keeps translation on this machine: a LibreTranslate server or local AI model on localhost qualifies.
pub fn translator_allowed(config: &Value, engine: &str) -> bool {
    mode(config) != "offline"
        || (engine == "libre"
            && is_local_url(text(config, "libreUrl", "https://libretranslate.com")))
        || (engine == "local"
            && is_local_url(text(config, "localBase", DEFAULT_LOCAL_BASE))
            && !text(config, "localModel", "").is_empty())
}

#[derive(Debug, PartialEq)]
pub enum Sse {
    Delta(String),
    Done,
}

/// Incremental decoder for `text/event-stream` bodies; tolerates chunks split anywhere, even mid-character.
#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Sse> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(payload) = line.trim().strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload == "[DONE]" {
                events.push(Sse::Done);
            } else if let Ok(value) = serde_json::from_str::<Value>(payload) {
                if let Some(delta) = value["choices"][0]["delta"]["content"].as_str() {
                    if !delta.is_empty() {
                        events.push(Sse::Delta(delta.into()));
                    }
                }
            }
        }
        events
    }
}

fn authorized(request: reqwest::RequestBuilder, key: Option<&str>) -> reqwest::RequestBuilder {
    match key {
        Some(key) if !key.is_empty() => request.bearer_auth(key),
        _ => request,
    }
}

fn http_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 | 403 => "Model từ chối khóa API. Hãy kiểm tra key trong Cài đặt.".into(),
        404 => "Không tìm thấy model hoặc địa chỉ. Hãy kiểm tra Cài đặt → AI.".into(),
        429 => "Model đang giới hạn lượt gọi. Hãy thử lại sau.".into(),
        code => format!("Model trả về HTTP {code}."),
    }
}

/// Streams a chat completion, calling `on_delta` per text fragment. Returns `Ok` when cancelled.
#[allow(clippy::too_many_arguments)]
pub async fn stream_chat(
    client: &reqwest::Client,
    base: &str,
    key: Option<&str>,
    model: &str,
    system: &str,
    messages: &Value,
    max_tokens: u64,
    cancel: Arc<Notify>,
    mut on_delta: impl FnMut(&str),
) -> Result<(), String> {
    validate_base(base)?;
    let mut all = vec![json!({"role": "system", "content": system})];
    all.extend(messages.as_array().cloned().unwrap_or_default());
    let request = authorized(
        client.post(format!("{}/chat/completions", base.trim_end_matches('/'))),
        key,
    )
    .timeout(std::time::Duration::from_secs(600))
    .json(&json!({"model": model, "messages": all, "stream": true, "max_tokens": max_tokens}));
    let mut response = tokio::select! {
        _ = cancel.notified() => return Ok(()),
        sent = request.send() => sent.map_err(|_| "Không kết nối được model. Hãy kiểm tra địa chỉ và dịch vụ đang chạy.".to_string())?,
    };
    if !response.status().is_success() {
        return Err(http_error(response.status()));
    }
    let mut decoder = SseDecoder::default();
    loop {
        let chunk = tokio::select! {
            _ = cancel.notified() => return Ok(()),
            chunk = response.chunk() => chunk.map_err(|_| "Kết nối model bị ngắt giữa chừng.".to_string())?,
        };
        let Some(chunk) = chunk else { return Ok(()) };
        for event in decoder.feed(&chunk) {
            match event {
                Sse::Delta(text) => on_delta(&text),
                Sse::Done => return Ok(()),
            }
        }
    }
}

/// Model ids offered by an OpenAI-compatible server (`GET {base}/models`).
pub async fn list_models(
    client: &reqwest::Client,
    base: &str,
    key: Option<&str>,
) -> Result<Vec<String>, String> {
    validate_base(base)?;
    let response = authorized(
        client.get(format!("{}/models", base.trim_end_matches('/'))),
        key,
    )
    .timeout(std::time::Duration::from_secs(15))
    .send()
    .await
    .map_err(|_| {
        "Không kết nối được model. Hãy kiểm tra địa chỉ và dịch vụ đang chạy.".to_string()
    })?;
    if !response.status().is_success() {
        return Err(http_error(response.status()));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "Phản hồi danh sách model không hợp lệ".to_string())?;
    let mut ids: Vec<String> = body["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|m| m["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    ids.truncate(200);
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn modes_choose_and_restrict_backends() {
        let remote = json!({"model": "claude-haiku-5-5"});
        assert_eq!(
            plan(&remote).unwrap(),
            vec![Backend::Claude {
                model: "claude-haiku-5-5".into()
            }]
        );

        let mut offline = json!({"mode": "offline", "localModel": "llama3"});
        assert_eq!(
            plan(&offline).unwrap(),
            vec![Backend::OpenAi {
                base: DEFAULT_LOCAL_BASE.into(),
                model: "llama3".into(),
                use_key: false
            }]
        );
        offline["localModel"] = json!("");
        assert!(plan(&offline).unwrap_err().contains("Offline"));
        // A primary that already points at this machine is allowed offline.
        let local_primary = json!({"mode": "offline", "chatProvider": "openai_compat", "openaiBase": "http://localhost:1234/v1", "openaiModel": "m"});
        assert!(plan(&local_primary).unwrap()[0].is_local());
        let remote_primary = json!({"mode": "offline", "chatProvider": "openai_compat", "openaiBase": "https://api.openai.com/v1", "openaiModel": "m"});
        assert!(plan(&remote_primary).is_err());

        let hybrid = json!({"mode": "hybrid", "localModel": "llama3"});
        let plan = plan(&hybrid).unwrap();
        assert_eq!(plan.len(), 2);
        assert!(matches!(plan[0], Backend::Claude { .. }) && plan[1].is_local());
        assert_eq!(super::plan(&json!({"mode": "hybrid"})).unwrap().len(), 1);
        let local_provider = json!({"chatProvider": "local", "localBase": "http://127.0.0.1:11434", "localModel": "gemma3:4b"});
        assert_eq!(
            super::plan(&local_provider).unwrap(),
            vec![Backend::OpenAi {
                base: "http://127.0.0.1:11434/v1".into(),
                model: "gemma3:4b".into(),
                use_key: false
            }]
        );
        assert!(super::plan(&json!({"chatProvider": "local", "localModel": ""})).is_err());

        assert!(super::plan(&json!({"modelSource": "cloudflare"}))
            .unwrap_err()
            .contains("cloud"));
        let cloud = json!({"modelSource": "cloudflare", "cloudUrl": "https://michiyo.example.workers.dev/", "cloudModel": "@cf/m"});
        assert_eq!(
            super::plan(&cloud).unwrap(),
            vec![Backend::Cloudflare {
                base: "https://michiyo.example.workers.dev/v1/ai".into(),
                model: "@cf/m".into()
            }]
        );
        // Offline only accepts a Worker running on this machine (e.g. `wrangler dev`).
        let offline_cloud = |url: &str| json!({"mode": "offline", "modelSource": "cloudflare", "cloudUrl": url, "localModel": "llama3"});
        assert!(matches!(
            super::plan(&offline_cloud("http://127.0.0.1:8787")).unwrap()[0],
            Backend::Cloudflare { .. }
        ));
        assert!(matches!(
            super::plan(&offline_cloud("https://x.workers.dev")).unwrap()[0],
            Backend::OpenAi { .. }
        ));
        assert!(super::plan(
            &json!({"modelSource": "cloudflare", "cloudUrl": "http://x.workers.dev"})
        )
        .is_err());
    }

    #[test]
    fn rejects_unsafe_or_incomplete_endpoints() {
        for base in ["http://example.com/v1", "ftp://x", "not a url"] {
            let c =
                json!({"chatProvider": "openai_compat", "openaiBase": base, "openaiModel": "m"});
            assert!(plan(&c).is_err(), "{base}");
        }
        let no_model =
            json!({"chatProvider": "openai_compat", "openaiBase": "https://api.openai.com/v1"});
        assert!(plan(&no_model).unwrap_err().contains("model"));
        assert!(is_local_url("http://localhost:11434/v1") && is_local_url("http://[::1]:8080"));
        assert!(!is_local_url("http://localhost.evil.com/v1"));
    }

    #[test]
    fn offline_translation_only_allows_local_libre() {
        let online = json!({});
        assert!(translator_allowed(&online, "google"));
        let offline = json!({"mode": "offline", "libreUrl": "http://127.0.0.1:5000"});
        assert!(translator_allowed(&offline, "libre"));
        let offline_local = json!({"mode": "offline", "localBase": "http://127.0.0.1:11434/v1", "localModel": "gemma3:4b"});
        assert!(translator_allowed(&offline_local, "local"));
        for engine in ["claude", "google", "mymemory"] {
            assert!(!translator_allowed(&offline, engine));
        }
        assert!(!translator_allowed(&json!({"mode": "offline"}), "libre"));
        assert!(!translator_allowed(&json!({"mode": "offline"}), "local"));
    }

    #[test]
    fn decoder_handles_split_chunks_and_multibyte_text() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Xin \"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"chào ✦\"}}]}\n\ndata: {\"choices\":[{\"delta\":{}}]}\n\ndata: [DONE]\n\n";
        let bytes = body.as_bytes();
        for split in 1..bytes.len() {
            let mut decoder = SseDecoder::default();
            let mut events = decoder.feed(&bytes[..split]);
            events.extend(decoder.feed(&bytes[split..]));
            assert_eq!(
                events,
                vec![
                    Sse::Delta("Xin ".into()),
                    Sse::Delta("chào ✦".into()),
                    Sse::Done
                ],
                "split at {split}"
            );
        }
    }

    fn sse(parts: &[&str]) -> String {
        parts
            .iter()
            .map(|p| {
                format!(
                    "data: {}\n\n",
                    json!({"choices": [{"delta": {"content": p}}]})
                )
            })
            .collect::<String>()
            + "data: [DONE]\n\n"
    }

    #[tokio::test]
    async fn streams_deltas_and_sends_key_and_system_prompt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(sse(&["Xin ", "chào"]), "text/event-stream"),
            )
            .mount(&server)
            .await;
        let base = format!("{}/v1", server.uri());
        let mut got = String::new();
        stream_chat(
            &reqwest::Client::new(),
            &base,
            Some("sk-test"),
            "m",
            "Bạn là Michiyo",
            &json!([{"role": "user", "content": "hi"}]),
            64,
            Arc::new(Notify::new()),
            |t| got.push_str(t),
        )
        .await
        .unwrap();
        assert_eq!(got, "Xin chào");
        let body: Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(
            body["messages"][0],
            json!({"role": "system", "content": "Bạn là Michiyo"})
        );
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 64);
    }

    #[tokio::test]
    async fn maps_http_errors_without_leaking_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key sk-secret"))
            .mount(&server)
            .await;
        let error = stream_chat(
            &reqwest::Client::new(),
            &server.uri(),
            Some("sk-secret"),
            "m",
            "",
            &json!([]),
            8,
            Arc::new(Notify::new()),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains("từ chối khóa") && !error.contains("sk-secret"));
        // Plain HTTP to a non-local host is refused before any request is made.
        assert!(stream_chat(
            &reqwest::Client::new(),
            "http://example.com/v1",
            None,
            "m",
            "",
            &json!([]),
            8,
            Arc::new(Notify::new()),
            |_| {}
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn cancellation_stops_a_slow_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_secs(5))
                    .set_body_raw(sse(&["late"]), "text/event-stream"),
            )
            .mount(&server)
            .await;
        let cancel = Arc::new(Notify::new());
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            trigger.notify_one();
        });
        let mut got = String::new();
        let started = std::time::Instant::now();
        stream_chat(
            &reqwest::Client::new(),
            &server.uri(),
            None,
            "m",
            "",
            &json!([]),
            8,
            cancel,
            |t| got.push_str(t),
        )
        .await
        .unwrap();
        assert!(got.is_empty() && started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[tokio::test]
    async fn lists_models_sorted_and_unique() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "b"}, {"id": "a"}, {"id": "b"}]})),
            )
            .mount(&server)
            .await;
        let ids = list_models(
            &reqwest::Client::new(),
            &format!("{}/v1", server.uri()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(ids, vec!["a", "b"]);
    }
}
