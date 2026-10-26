use serde_json::Value;
use tauri_plugin_store::StoreExt;
pub fn read(app: &tauri::AppHandle) -> Result<Value, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(store.get("config").unwrap_or_else(|| serde_json::json!({})))
}
pub fn string(c: &Value, k: &str, default: &str) -> String {
    c[k].as_str().unwrap_or(default).into()
}
