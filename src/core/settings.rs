use std::path::{Path, PathBuf};

use super::{dpapi, ini::IniFile};

pub const YEARS: [&str; 12] = ["2023", "2024", "2025", "2026", "2027", "2028", "2029", "2030", "2031", "2032", "2033", "2034"];
pub const MONTHS: [&str; 12] = ["01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12"];

/// 視窗位置與大小（egui 邏輯座標 points）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

/// 跨程式重啟保留的設定。密碼僅在 `remember_password` 時以 DPAPI 加密儲存。
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    // 首次安裝不預填伺服器、帳號、工作目錄與 Excel 清單，由使用者自行輸入/選擇後記憶於 config.ini
    pub smtp_server: String,
    pub smtp_port: u16,
    pub email_account: String,
    /// 略過 SSL 憑證驗證。C++ 版固定關閉驗證，為讓既有郵件伺服器照常可用預設維持 true。
    pub skip_cert_validation: bool,
    pub password: String,
    pub remember_password: bool,
    pub global_extra: String,
    pub work_dir: String,
    pub xlsx_path: String,
    /// 預設為相對路徑 PaySlips，以工作目錄（未設定時為程式目錄）為基準。
    pub pdf_folder: String,
    pub year_idx: usize,
    pub month_idx: usize,
    pub interval_seconds: u32,
    pub main_window: Option<WindowBounds>,
    pub editor_window: Option<WindowBounds>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            smtp_server: String::new(),
            smtp_port: 465,
            email_account: String::new(),
            skip_cert_validation: true,
            password: String::new(),
            remember_password: true,
            global_extra: String::new(),
            work_dir: String::new(),
            xlsx_path: String::new(),
            pdf_folder: "PaySlips".into(),
            year_idx: 0,
            month_idx: 0,
            interval_seconds: 15,
            main_window: None,
            editor_window: None,
        }
    }
}

impl AppSettings {
    pub fn year(&self) -> &'static str {
        YEARS[self.year_idx.min(YEARS.len() - 1)]
    }

    pub fn month(&self) -> &'static str {
        MONTHS[self.month_idx.min(MONTHS.len() - 1)]
    }
}

const SECTION: &str = "Settings";

/// 讀寫 config.ini，與 C++ / .NET 版共用同一份檔案與鍵名。
/// 視窗位置使用獨立鍵名（RsMainWindow / RsEditorWindow），因單位與 .NET 版（實體像素）不同。
pub struct SettingsStore {
    ini_path: PathBuf,
}

impl SettingsStore {
    pub fn new(data_dir: &Path) -> Self {
        Self { ini_path: data_dir.join("config.ini") }
    }

    pub fn ini_path(&self) -> &Path {
        &self.ini_path
    }

    /// 載入設定；檔案不存在或欄位缺漏時保留 `defaults` 的值。
    pub fn load(&self, defaults: AppSettings) -> std::io::Result<AppSettings> {
        if !self.ini_path.exists() {
            return Ok(defaults);
        }
        let ini = IniFile::load(&self.ini_path)?;
        let get = |k: &str| ini.get(SECTION, k);
        let s = |k: &str, d: &String| get(k).filter(|v| !v.is_empty()).map_or_else(|| d.clone(), str::to_string);
        let b = |k: &str, d: bool| match get(k) {
            Some("1") => true,
            Some("0") => false,
            _ => d,
        };
        fn num<T: std::str::FromStr>(v: Option<&str>, d: T) -> T {
            v.and_then(|v| v.trim().parse().ok()).unwrap_or(d)
        }

        Ok(AppSettings {
            smtp_server: s("SmtpServer", &defaults.smtp_server),
            smtp_port: num(get("SmtpPort"), defaults.smtp_port),
            email_account: s("EmailAccount", &defaults.email_account),
            skip_cert_validation: b("SkipCertValidation", defaults.skip_cert_validation),
            remember_password: b("RememberPassword", defaults.remember_password),
            password: get("Password").and_then(dpapi::unprotect).unwrap_or(defaults.password),
            global_extra: get("GlobalExtra").map_or(defaults.global_extra, str::to_string),
            work_dir: s("WorkDir", &defaults.work_dir),
            xlsx_path: s("XlsxPath", &defaults.xlsx_path),
            pdf_folder: s("PdfFolder", &defaults.pdf_folder),
            year_idx: num(get("YearIdx"), defaults.year_idx).min(YEARS.len() - 1),
            month_idx: num(get("MonthIdx"), defaults.month_idx).min(MONTHS.len() - 1),
            interval_seconds: num(get("Interval"), defaults.interval_seconds).max(1),
            main_window: get("RsMainWindow").and_then(parse_bounds).or(defaults.main_window),
            editor_window: get("RsEditorWindow").and_then(parse_bounds).or(defaults.editor_window),
        })
    }

    pub fn save(&self, s: &AppSettings) -> std::io::Result<()> {
        // 先讀回既有內容再覆寫，保留其他版本寫入的鍵
        let mut ini = IniFile::load(&self.ini_path)?;
        let mut set = |k: &str, v: &str| ini.set(SECTION, k, v);
        set("WorkDir", &s.work_dir);
        set("XlsxPath", &s.xlsx_path);
        set("PdfFolder", &s.pdf_folder);
        set("YearIdx", &s.year_idx.to_string());
        set("MonthIdx", &s.month_idx.to_string());
        set("Interval", &s.interval_seconds.to_string());
        set("SmtpServer", &s.smtp_server);
        set("SmtpPort", &s.smtp_port.to_string());
        set("EmailAccount", &s.email_account);
        set("SkipCertValidation", if s.skip_cert_validation { "1" } else { "0" });
        set("RememberPassword", if s.remember_password { "1" } else { "0" });
        let stored = if s.remember_password && !s.password.is_empty() {
            dpapi::protect(&s.password).unwrap_or_default()
        } else {
            String::new()
        };
        set("Password", &stored);
        set("GlobalExtra", &s.global_extra);
        set("RsMainWindow", &format_bounds(s.main_window));
        set("RsEditorWindow", &format_bounds(s.editor_window));
        ini.save(&self.ini_path)
    }
}

pub(crate) fn format_bounds(b: Option<WindowBounds>) -> String {
    b.map_or_else(String::new, |b| format!("{},{},{},{},{}", b.x, b.y, b.width, b.height, u8::from(b.maximized)))
}

pub(crate) fn parse_bounds(text: &str) -> Option<WindowBounds> {
    let p: Vec<&str> = text.split(',').map(str::trim).collect();
    let [x, y, w, h, m] = p.as_slice() else { return None };
    let (x, y, width, height) = (x.parse().ok()?, y.parse().ok()?, w.parse::<f32>().ok()?, h.parse::<f32>().ok()?);
    (width > 0.0 && height > 0.0).then_some(WindowBounds { x, y, width, height, maximized: *m == "1" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_has_no_preset_smtp_account_or_paths() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::new(dir.path()).load(AppSettings::default()).unwrap();
        assert_eq!(s.smtp_server, "");
        assert_eq!(s.email_account, "");
        assert_eq!(s.password, "");
        assert_eq!(s.work_dir, "");
        assert_eq!(s.xlsx_path, "");
        assert_eq!(s.pdf_folder, "PaySlips");
        assert_eq!(s.global_extra, "");
    }

    #[test]
    fn round_trip_all_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        std::fs::write(store.ini_path(), "[Settings]\r\nLegacyKey=keep\r\nYearIdx=99\r\n").unwrap();
        assert_eq!(store.load(AppSettings::default()).unwrap().year_idx, YEARS.len() - 1);

        let saved = AppSettings {
            smtp_server: "smtp.example.com".into(),
            smtp_port: 587,
            email_account: "hr@example.com".into(),
            skip_cert_validation: false,
            password: "p@ss 密碼".into(),
            global_extra: "H:\\薪資\\公告.pdf".into(),
            work_dir: "H:\\薪資\\2026.08".into(),
            xlsx_path: "H:\\薪資\\2026.08\\passwords.xlsx".into(),
            year_idx: 3,
            month_idx: 8,
            interval_seconds: 7,
            main_window: Some(WindowBounds { x: -1200.0, y: 40.5, width: 1400.0, height: 900.0, maximized: true }),
            editor_window: Some(WindowBounds { x: 100.0, y: 80.0, width: 800.0, height: 700.0, maximized: false }),
            ..AppSettings::default()
        };
        store.save(&saved).unwrap();

        let raw = super::super::ini::decode(&std::fs::read(store.ini_path()).unwrap());
        assert!(!raw.contains("p@ss"), "密碼不得以明文存檔");
        assert!(raw.contains("LegacyKey=keep"));

        assert_eq!(store.load(AppSettings::default()).unwrap(), saved);
    }

    #[test]
    fn password_cleared_when_remember_off() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        store.save(&AppSettings { password: "secret".into(), ..Default::default() }).unwrap();
        store
            .save(&AppSettings { password: "secret".into(), remember_password: false, ..Default::default() })
            .unwrap();
        let loaded = store.load(AppSettings::default()).unwrap();
        assert_eq!(loaded.password, "");
        assert!(!loaded.remember_password);
    }

    #[test]
    fn invalid_bounds_are_ignored() {
        for t in ["", "1,2,3", "a,b,c,d,0", "0,0,0,500,0"] {
            assert_eq!(parse_bounds(t), None, "{t}");
        }
    }
}
