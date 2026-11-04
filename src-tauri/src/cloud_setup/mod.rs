pub mod cloudflare;

use cloudflare::Cf;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const MANIFEST_JSON: &str = include_str!("../../resources/cloud-backend/manifest.json");
pub const WORKER_JS: &[u8] = include_bytes!("../../resources/cloud-backend/worker/index.js");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub main_module: String,
    pub compatibility_date: String,
    pub compatibility_flags: Vec<String>,
    pub vars: HashMap<String, String>,
    pub migrations: Vec<String>,
    pub assets: Vec<AssetEntry>,
    pub assets_config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetEntry {
    pub path: String,
    pub hash: String,
    pub size: usize,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployProgress {
    pub step: String,
    pub status: String, // "running" | "done" | "error"
    pub message: String,
}

fn random_secret(bytes_len: usize) -> String {
    use rand::RngCore;
    let mut bytes = vec![0u8; bytes_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(&bytes)
}

fn random_base64_secret(bytes_len: usize) -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = vec![0u8; bytes_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }
}

pub async fn run_migrations(
    cf: &Cf<'_>,
    account_id: &str,
    d1_id: &str,
    _manifest: &Manifest,
    progress: &impl Fn(DeployProgress),
) -> Result<(), String> {
    progress(DeployProgress {
        step: "d1_migrations".into(),
        status: "running".into(),
        message: "Đang kiểm tra và áp dụng migration D1...".into(),
    });

    // Create d1_migrations table if not exists (same schema as wrangler)
    let init_sql = r#"
        CREATE TABLE IF NOT EXISTS d1_migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
    "#;
    cf.d1_query(account_id, d1_id, init_sql).await?;

    let applied_rows = cf
        .d1_query(account_id, d1_id, "SELECT name FROM d1_migrations")
        .await?;
    let mut applied_names: Vec<String> = Vec::new();
    if let Some(first_stmt) = applied_rows.first() {
        if let Some(arr) = first_stmt.as_array() {
            for row in arr {
                if let Some(name) = row["name"].as_str() {
                    applied_names.push(name.to_string());
                }
            }
        }
    }

    // Embed all migrations
    let migration_files: &[(&str, &str)] = &[
        ("0001_baseline.sql", include_str!("../../resources/cloud-backend/migrations/0001_baseline.sql")),
        ("0002_owner_sessions.sql", include_str!("../../resources/cloud-backend/migrations/0002_owner_sessions.sql")),
    ];

    for &(filename, content) in migration_files {
        if !applied_names.contains(&filename.to_string()) {
            progress(DeployProgress {
                step: "d1_migrations".into(),
                status: "running".into(),
                message: format!("Đang chạy migration {}...", filename),
            });
            cf.d1_query(account_id, d1_id, content).await?;
            let record_sql = format!(
                "INSERT INTO d1_migrations (name) VALUES ('{}');",
                filename.replace('\'', "''")
            );
            cf.d1_query(account_id, d1_id, &record_sql).await?;
        }
    }

    progress(DeployProgress {
        step: "d1_migrations".into(),
        status: "done".into(),
        message: "Cơ sở dữ liệu D1 đã sẵn sàng.".into(),
    });
    Ok(())
}

pub async fn deploy_worker(
    cf: &Cf<'_>,
    account_id: &str,
    d1_id: &str,
    manifest: &Manifest,
    progress: &impl Fn(DeployProgress),
) -> Result<(String, String), String> {
    // 1. Ensure workers.dev subdomain
    progress(DeployProgress {
        step: "subdomain".into(),
        status: "running".into(),
        message: "Đang kiểm tra tên miền workers.dev...".into(),
    });
    let default_sub = format!("michiyo-{}", &account_id[..6]);
    let subdomain = cf.ensure_workers_subdomain(account_id, &default_sub).await?;
    let worker_url = format!("https://michiyo-cloud.{}.workers.dev", subdomain);

    // 2. Upload Static Assets session if any
    let mut asset_jwt: Option<String> = None;
    if !manifest.assets.is_empty() {
        progress(DeployProgress {
            step: "assets".into(),
            status: "running".into(),
            message: "Đang chuẩn bị phiên tải lên web quản trị (static assets)...".into(),
        });

        let mut manifest_map = serde_json::Map::new();
        for a in &manifest.assets {
            manifest_map.insert(
                a.path.clone(),
                json!({
                    "hash": a.hash,
                    "size": a.size
                }),
            );
        }

        let session_res = cf
            .post(
                &format!("/accounts/{}/workers/scripts/michiyo-cloud/assets-upload-session", account_id),
                &json!({ "manifest": manifest_map }),
            )
            .await;

        if let Ok(res) = session_res {
            if let Some(jwt) = res["jwt"].as_str() {
                asset_jwt = Some(jwt.to_string());
            }
        }
    }

    // 3. Secrets: check if ENCRYPTION_KEY exists
    progress(DeployProgress {
        step: "secrets".into(),
        status: "running".into(),
        message: "Đang thiết lập khóa bảo mật (ENCRYPTION_KEY, BOOTSTRAP_SECRET)...".into(),
    });
    let existing_secrets = cf
        .secret_names(account_id, "michiyo-cloud")
        .await
        .unwrap_or_default();

    if !existing_secrets.contains(&"ENCRYPTION_KEY".to_string()) {
        let enc_key = random_base64_secret(32);
        cf.put_secret(account_id, "michiyo-cloud", "ENCRYPTION_KEY", &enc_key).await?;
    }

    let bootstrap_secret = random_secret(24);
    cf.put_secret(account_id, "michiyo-cloud", "BOOTSTRAP_SECRET", &bootstrap_secret).await?;

    // 4. Worker Script Upload (multipart)
    progress(DeployProgress {
        step: "upload".into(),
        status: "running".into(),
        message: "Đang tải lên mã nguồn Worker backend...".into(),
    });

    let mut bindings = vec![
        json!({
            "name": "DB",
            "type": "d1",
            "id": d1_id
        }),
        json!({
            "name": "AI",
            "type": "ai"
        }),
    ];

    if asset_jwt.is_some() {
        bindings.push(json!({
            "name": "ASSETS",
            "type": "assets"
        }));
    }

    for (k, v) in &manifest.vars {
        bindings.push(json!({
            "name": k,
            "type": "plain_text",
            "text": v
        }));
    }

    let mut metadata = json!({
        "main_module": manifest.main_module,
        "compatibility_date": manifest.compatibility_date,
        "compatibility_flags": manifest.compatibility_flags,
        "bindings": bindings,
        "keep_bindings": ["secret_text"]
    });

    if let Some(jwt) = asset_jwt {
        metadata["assets"] = json!({
            "jwt": jwt,
            "config": manifest.assets_config
        });
    }

    let form = reqwest::multipart::Form::new()
        .text("metadata", metadata.to_string())
        .part(
            "index.js",
            reqwest::multipart::Part::bytes(WORKER_JS.to_vec())
                .file_name("index.js")
                .mime_str("application/javascript+module")
                .unwrap(),
        );

    cf.multipart(
        reqwest::Method::PUT,
        &format!("/accounts/{}/workers/scripts/michiyo-cloud", account_id),
        &cf.token,
        form,
    )
    .await?;

    // 5. Enable workers.dev subdomain for script
    progress(DeployProgress {
        step: "enable_subdomain".into(),
        status: "running".into(),
        message: "Đang kích hoạt URL workers.dev...".into(),
    });
    cf.enable_script_subdomain(account_id, "michiyo-cloud").await?;

    progress(DeployProgress {
        step: "upload".into(),
        status: "done".into(),
        message: "Worker đã được triển khai thành công!".into(),
    });

    Ok((worker_url, bootstrap_secret))
}

pub async fn bootstrap_pairing(
    client: &reqwest::Client,
    worker_url: &str,
    bootstrap_secret: &str,
    device_name: &str,
) -> Result<(String, String), String> {
    // Retry a few times as DNS / edge propagation for new worker might take a few seconds
    let endpoint = format!("{}/v1/pair/bootstrap", worker_url.trim_end_matches('/'));
    let mut last_err = String::new();

    for attempt in 1..=8 {
        tokio::time::sleep(std::time::Duration::from_millis(if attempt == 1 { 800 } else { 2000 })).await;
        let res = client
            .post(&endpoint)
            .json(&json!({
                "secret": bootstrap_secret,
                "device_name": device_name
            }))
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await;

        match res {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
                    let token = body["token"].as_str().ok_or("Thiếu token trong phản hồi bootstrap")?;
                    let device_id = body["device_id"].as_str().unwrap_or_default();
                    return Ok((device_id.to_string(), token.to_string()));
                } else {
                    let err_txt = resp.text().await.unwrap_or_default();
                    last_err = format!("HTTP {}: {}", status, err_txt);
                }
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    Err(format!("Không thể ghép nối tự động với backend: {}", last_err))
}
