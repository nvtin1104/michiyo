use base64::Engine;
use serde_json::{json, Value};
use tauri::Manager;
fn root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("skins");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}
#[tauri::command]
pub fn list_skins(app: tauri::AppHandle) -> Result<Value, String> {
    let root = root(&app)?;
    let mut names = vec!["default".to_string()];
    for entry in std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        if entry.path().join("manifest.json").is_file()
            && entry.path().join("spritesheet.png").is_file()
        {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    Ok(json!({"names":names,"directory":root.to_string_lossy()}))
}
#[tauri::command]
pub fn load_skin(app: tauri::AppHandle, name: String) -> Result<Value, String> {
    if name == "default" || name.contains(['/', '\\', '.']) {
        return Err("Tên skin không hợp lệ".into());
    }
    let root = root(&app)?.canonicalize().map_err(|e| e.to_string())?;
    let folder = root
        .join(&name)
        .canonicalize()
        .map_err(|_| "Không tìm thấy skin")?;
    if !folder.starts_with(root) {
        return Err("Skin nằm ngoài thư mục cho phép".into());
    }
    let data = std::fs::read(folder.join("manifest.json")).map_err(|e| e.to_string())?;
    if data.len() > 65536 {
        return Err("Manifest quá lớn".into());
    }
    let manifest: Value =
        serde_json::from_slice(&data).map_err(|_| "Manifest JSON không hợp lệ")?;
    let image = std::fs::read(folder.join("spritesheet.png")).map_err(|e| e.to_string())?;
    if image.len() > 8 * 1024 * 1024 {
        return Err("Skin tối đa 8MB".into());
    }
    Ok(
        json!({"manifest":manifest,"image":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(image))}),
    )
}
