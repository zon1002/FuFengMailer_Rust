pub mod dpapi;
pub mod employee;
pub mod ini;
pub mod loader;
pub mod mail_content;
pub mod mailer;
pub mod settings;
pub mod templates;

use std::path::PathBuf;

/// 使用者可寫入的資料目錄：%APPDATA%\FuFengMailer_Rust。
/// 刻意不與 C++ / .NET 版的 %APPDATA%\FuFengMailer 共用，安裝後首次執行時所有欄位皆為空白，
/// 不會沿用其他版本留下的伺服器、帳號、密碼、路徑與範本。
pub fn default_data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Into::into)).unwrap_or_default());
    base.join("FuFengMailer_Rust")
}

/// 將檔名中 Windows 不允許的字元換成底線。
pub fn safe_file_name(name: &str) -> String {
    name.chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '_' } else { c })
        .collect()
}
