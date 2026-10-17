pub mod dpapi;
pub mod employee;
pub mod ini;
pub mod loader;
pub mod mail_content;
pub mod mailer;
pub mod settings;
pub mod templates;

use std::path::PathBuf;

/// 使用者可寫入的資料目錄：%APPDATA%\FuFengMailer（與 C++、.NET 版共用）。
pub fn default_data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Into::into)).unwrap_or_default());
    base.join("FuFengMailer")
}

/// 將檔名中 Windows 不允許的字元換成底線。
pub fn safe_file_name(name: &str) -> String {
    name.chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '_' } else { c })
        .collect()
}
