//! Minimal Cloudflare REST v4 client used to provision the user's own Michiyo backend.
//! Error messages never contain the token; Cloudflare's own error texts are passed through
//! because they explain missing permissions precisely.
use serde_json::{json, Value};

pub struct Cf<'a> {
    pub client: &'a reqwest::Client,
    pub base: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

fn describe(status: reqwest::StatusCode, body: &Value) -> String {
    let detail = body["errors"]
        .as_array()
        .and_then(|e| e.first())
        .map(|e| {
            format!(
                "{} (mã {})",
                e["message"].as_str().unwrap_or("lỗi không rõ"),
                e["code"].as_i64().unwrap_or_default()
            )
        })
        .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
    match status.as_u16() {
        401 => format!("Cloudflare từ chối token: {detail}. Hãy kiểm tra lại token."),
        403 => format!("Token Cloudflare thiếu quyền: {detail}."),
        _ => format!("Cloudflare báo lỗi: {detail}"),
    }
}

/// Cloudflare error code returned when an account has no workers.dev subdomain yet.
pub const NO_SUBDOMAIN: i64 = 10007;

#[derive(Debug)]
pub struct CfError {
    pub status: u16,
    pub codes: Vec<i64>,
    pub message: String,
}

impl From<CfError> for String {
    fn from(e: CfError) -> String {
        e.message
    }
}

impl<'a> Cf<'a> {
    pub fn new(client: &'a reqwest::Client, base: &str, token: &str) -> Self {
        Self {
            client,
            base: base.trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn finish(&self, sent: Result<reqwest::Response, reqwest::Error>) -> Result<Value, CfError> {
        let response = sent.map_err(|_| CfError {
            status: 0,
            codes: vec![],
            message: "Không kết nối được Cloudflare. Hãy kiểm tra mạng.".into(),
        })?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() && body["success"].as_bool() != Some(false) {
            return Ok(body["result"].clone());
        }
        Err(CfError {
            status: status.as_u16(),
            codes: body["errors"]
                .as_array()
                .map(|e| e.iter().filter_map(|x| x["code"].as_i64()).collect())
                .unwrap_or_default(),
            message: describe(status, &body),
        })
    }

    pub async fn get(&self, path: &str) -> Result<Value, CfError> {
        let sent = self.client.get(self.url(path)).bearer_auth(&self.token).send().await;
        self.finish(sent).await
    }

    pub async fn send_json(&self, method: reqwest::Method, path: &str, body: &Value) -> Result<Value, CfError> {
        let sent = self
            .client
            .request(method, self.url(path))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await;
        self.finish(sent).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value, CfError> {
        self.send_json(reqwest::Method::POST, path, body).await
    }

    pub async fn put(&self, path: &str, body: &Value) -> Result<Value, CfError> {
        self.send_json(reqwest::Method::PUT, path, body).await
    }

    /// Multipart request authenticated with `bearer` (the API token, or an assets upload JWT).
    pub async fn multipart(
        &self,
        method: reqwest::Method,
        path: &str,
        bearer: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Value, CfError> {
        let sent = self
            .client
            .request(method, self.url(path))
            .bearer_auth(bearer)
            .timeout(std::time::Duration::from_secs(180))
            .multipart(form)
            .send()
            .await;
        self.finish(sent).await
    }

    pub async fn accounts(&self) -> Result<Vec<Account>, String> {
        let result = self.get("/accounts?per_page=50").await?;
        let accounts: Vec<Account> = result
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|a| {
                        Some(Account {
                            id: a["id"].as_str()?.to_string(),
                            name: a["name"].as_str().unwrap_or("Cloudflare").to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if accounts.is_empty() {
            return Err("Token không truy cập được tài khoản Cloudflare nào. Hãy chọn \"All accounts\" hoặc một tài khoản khi tạo token.".into());
        }
        Ok(accounts)
    }

    /// Text-generation model ids available to Workers AI on `account`. Also proves the token
    /// carries the Workers AI permission.
    pub async fn text_models(&self, account: &str) -> Result<Vec<String>, String> {
        let result = self
            .get(&format!(
                "/accounts/{account}/ai/models/search?task=Text%20Generation&per_page=100"
            ))
            .await?;
        let mut names: Vec<String> = result
            .as_array()
            .map(|m| m.iter().filter_map(|x| x["name"].as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Finds the D1 database named `name`, creating it when missing. Returns its uuid.
    pub async fn ensure_d1(&self, account: &str, name: &str) -> Result<String, String> {
        let found = self
            .get(&format!("/accounts/{account}/d1/database?name={name}&per_page=100"))
            .await?;
        if let Some(id) = found.as_array().and_then(|list| {
            list.iter()
                .find(|d| d["name"].as_str() == Some(name))
                .and_then(|d| d["uuid"].as_str())
        }) {
            return Ok(id.to_string());
        }
        let created = self
            .post(&format!("/accounts/{account}/d1/database"), &json!({ "name": name }))
            .await?;
        created["uuid"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Cloudflare không trả về mã cơ sở dữ liệu D1".into())
    }

    /// Runs SQL (may contain several statements) against D1 and returns the rows of each statement.
    pub async fn d1_query(&self, account: &str, db: &str, sql: &str) -> Result<Vec<Value>, String> {
        let result = self
            .post(
                &format!("/accounts/{account}/d1/database/{db}/query"),
                &json!({ "sql": sql }),
            )
            .await?;
        Ok(result
            .as_array()
            .map(|r| r.iter().map(|s| s["results"].clone()).collect())
            .unwrap_or_default())
    }

    pub async fn secret_names(&self, account: &str, script: &str) -> Result<Vec<String>, CfError> {
        let result = self
            .get(&format!("/accounts/{account}/workers/scripts/{script}/secrets"))
            .await?;
        Ok(result
            .as_array()
            .map(|s| s.iter().filter_map(|x| x["name"].as_str().map(str::to_owned)).collect())
            .unwrap_or_default())
    }

    pub async fn put_secret(&self, account: &str, script: &str, name: &str, value: &str) -> Result<(), String> {
        self.put(
            &format!("/accounts/{account}/workers/scripts/{script}/secrets"),
            &json!({ "name": name, "text": value, "type": "secret_text" }),
        )
        .await?;
        Ok(())
    }

    /// The account's `<subdomain>.workers.dev`, creating `fallback` when the account has none.
    pub async fn ensure_workers_subdomain(&self, account: &str, fallback: &str) -> Result<String, String> {
        match self.get(&format!("/accounts/{account}/workers/subdomain")).await {
            Ok(r) if r["subdomain"].as_str().is_some_and(|s| !s.is_empty()) => {
                Ok(r["subdomain"].as_str().unwrap().to_string())
            }
            Ok(_) => self.create_subdomain(account, fallback).await,
            Err(e) if e.codes.contains(&NO_SUBDOMAIN) || e.status == 404 => {
                self.create_subdomain(account, fallback).await
            }
            Err(e) => Err(e.message),
        }
    }

    async fn create_subdomain(&self, account: &str, name: &str) -> Result<String, String> {
        let r = self
            .put(
                &format!("/accounts/{account}/workers/subdomain"),
                &json!({ "subdomain": name }),
            )
            .await?;
        Ok(r["subdomain"].as_str().unwrap_or(name).to_string())
    }

    pub async fn enable_script_subdomain(&self, account: &str, script: &str) -> Result<(), String> {
        self.post(
            &format!("/accounts/{account}/workers/scripts/{script}/subdomain"),
            &json!({ "enabled": true, "previews_enabled": false }),
        )
        .await?;
        Ok(())
    }
}
