pub fn get(name: &str) -> Result<String, String> {
    keyring::Entry::new("app.michiyo.desktop", name)
        .map_err(|_| "Không mở được keychain")?
        .get_password()
        .map_err(|_| format!("Chưa có API key cho {name}. Hãy mở Cài đặt."))
}
pub fn set(name: &str, value: &str) -> Result<(), String> {
    if !["claude", "google", "libre"].contains(&name) {
        return Err("Tên dịch vụ không hợp lệ".into());
    }
    let entry = keyring::Entry::new("app.michiyo.desktop", name).map_err(|e| e.to_string())?;
    if value.is_empty() {
        let _ = entry.delete_credential();
        Ok(())
    } else {
        entry.set_password(value).map_err(|e| e.to_string())
    }
}
