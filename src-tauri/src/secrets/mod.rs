pub fn get(name: &str) -> Result<String, String> {
    keyring::Entry::new("app.michiyo.desktop", name)
        .map_err(|_| "Không mở được keychain")?
        .get_password()
        .map_err(|error|match error{keyring::Error::NoEntry=>format!("Chưa có API key cho {name}. Hãy mở Cài đặt."),_=>"Không đọc được kho bảo mật hệ thống. Hãy kiểm tra quyền truy cập keychain và thử lại.".into()})
}
pub fn set(name: &str, value: &str) -> Result<(), String> {
    if !["claude", "google", "libre"].contains(&name) {
        return Err("Tên dịch vụ không hợp lệ".into());
    }
    let entry = keyring::Entry::new("app.michiyo.desktop", name).map_err(|e| e.to_string())?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Không xóa được API key trong kho bảo mật hệ thống.".into()),
        }
    } else {
        entry.set_password(value).map_err(|e| e.to_string())
    }
}
