//! 極簡 INI 讀寫，與 C++ 版（Win32 Profile API）及 .NET 版互通：
//! - 讀取：依 BOM 判斷 UTF-16/UTF-8；無 BOM 時先以嚴格 UTF-8 解碼，失敗才退回系統 ANSI 字碼頁
//!   （C++ 版首次建立的 config.ini 是 ANSI，例如繁中 Windows 的 CP950）。
//! - 寫入：一律 UTF-16 LE + BOM。

use std::io;
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct IniFile {
    /// 保留原始順序，方便與其他版本寫入的檔案比對
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl IniFile {
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut ini = Self::default();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(ini),
            Err(e) => return Err(e),
        };
        let mut section: Option<String> = None;
        for raw in decode(&bytes).split('\n') {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = Some(line[1..line.len() - 1].trim().to_string());
                continue;
            }
            if let (Some(sec), Some((k, v))) = (&section, line.split_once('='))
                && !k.trim().is_empty()
            {
                ini.set(sec, k.trim(), v.trim());
            }
        }
        Ok(ini)
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(s, _)| s.eq_ignore_ascii_case(section))?
            .1
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let idx = match self.sections.iter().position(|(s, _)| s.eq_ignore_ascii_case(section)) {
            Some(i) => i,
            None => {
                self.sections.push((section.to_string(), Vec::new()));
                self.sections.len() - 1
            }
        };
        let entries = &mut self.sections[idx].1;
        match entries.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(entry) => entry.1 = value.to_string(),
            None => entries.push((key.to_string(), value.to_string())),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut text = String::new();
        for (name, entries) in &self.sections {
            text.push_str(&format!("[{name}]\r\n"));
            for (k, v) in entries {
                text.push_str(&format!("{k}={v}\r\n"));
            }
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(path, bytes)
    }
}

pub(crate) fn decode(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8], le: bool| {
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => decode_ansi(bytes),
        },
    }
}

/// 以系統 ANSI 字碼頁（CP_ACP）解碼。
#[cfg(windows)]
fn decode_ansi(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{CP_ACP, MultiByteToWideChar};
    if bytes.is_empty() {
        return String::new();
    }
    let len = i32::try_from(bytes.len()).unwrap_or(i32::MAX);
    // SAFETY: 緩衝區長度由第一次呼叫取得，第二次呼叫寫入不會超過該長度
    unsafe {
        let n = MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut buf = vec![0u16; n as usize];
        MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, buf.as_mut_ptr(), n);
        String::from_utf16_lossy(&buf)
    }
}

#[cfg(not(windows))]
fn decode_ansi(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "[Settings]\r\nWorkDir=H:\\薪資\\2026.08\r\n";

    #[test]
    fn decodes_utf16_and_utf8() {
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(TEXT.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(&utf16), TEXT);
        assert_eq!(decode(TEXT.as_bytes()), TEXT);
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(TEXT.as_bytes());
        assert_eq!(decode(&bom), TEXT);
    }

    /// 以系統 ANSI 字碼頁編碼（模擬 C++ 版 WritePrivateProfileStringW 建立的 config.ini）
    #[cfg(windows)]
    fn encode_ansi(text: &str) -> Vec<u8> {
        use windows_sys::Win32::Globalization::{CP_ACP, WideCharToMultiByte};
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let n = WideCharToMultiByte(CP_ACP, 0, wide.as_ptr(), wide.len() as i32, std::ptr::null_mut(), 0, std::ptr::null(), std::ptr::null_mut());
            let mut buf = vec![0u8; n as usize];
            WideCharToMultiByte(CP_ACP, 0, wide.as_ptr(), wide.len() as i32, buf.as_mut_ptr(), n, std::ptr::null(), std::ptr::null_mut());
            buf
        }
    }

    #[cfg(windows)]
    #[test]
    fn decodes_ansi_code_page() {
        let bytes = encode_ansi(TEXT);
        // 系統字碼頁無法表示中文（例如英文版 Windows）或本身就是 UTF-8 時略過
        if bytes != TEXT.as_bytes() && std::str::from_utf8(&bytes).is_err() {
            assert_eq!(decode(&bytes), TEXT);
        }
    }

    #[test]
    fn round_trip_keeps_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.ini");
        std::fs::write(&path, "[Settings]\r\nLegacy=keep\r\n").unwrap();
        let mut ini = IniFile::load(&path).unwrap();
        ini.set("settings", "WorkDir", "H:\\薪資");
        ini.save(&path).unwrap();

        let again = IniFile::load(&path).unwrap();
        assert_eq!(again.get("Settings", "legacy"), Some("keep"));
        assert_eq!(again.get("Settings", "WorkDir"), Some("H:\\薪資"));
        assert_eq!(&std::fs::read(&path).unwrap()[..2], &[0xFF, 0xFE]);
    }
}
