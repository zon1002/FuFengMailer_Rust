//! 郵件內容與範本庫：
//! - mailContent.default：目前（本次及往後）寄送使用的內容
//! - MailTemplates\*.template：可自由套用/另存/刪除的範本庫，套用後修改不會回寫範本
//!
//! 檔案格式皆為 UTF-8，第一行為主旨、其餘為本文，與 C++ / .NET 版相容。

use std::io;
use std::path::{Path, PathBuf};

use super::mail_content::MailContent;

const EXTENSION: &str = "template";

pub struct TemplateStore {
    current_path: PathBuf,
    templates_dir: PathBuf,
}

impl TemplateStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            current_path: data_dir.join("mailContent.default"),
            templates_dir: data_dir.join("MailTemplates"),
        }
    }

    /// 讀取目前寄送內容；檔案不存在時以預設內容建立。
    pub fn load_current(&self) -> io::Result<MailContent> {
        if !self.current_path.exists() {
            let d = MailContent::default_content();
            self.save_current(&d)?;
            return Ok(d);
        }
        let mut c = MailContent::parse(&read_text(&self.current_path)?);
        // C++ 版行為：本文為空時沿用預設本文
        if c.body.is_empty() {
            c.body = MailContent::default_content().body;
        }
        Ok(c)
    }

    pub fn save_current(&self, content: &MailContent) -> io::Result<()> {
        if let Some(dir) = self.current_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.current_path, content.serialize())
    }

    /// 範本庫為空時，以 `seed` 播種一份「預設」範本。
    pub fn ensure_templates_dir(&self, seed: &MailContent) -> io::Result<()> {
        std::fs::create_dir_all(&self.templates_dir)?;
        if self.list()?.is_empty() {
            self.save_as("預設", seed)?;
        }
        Ok(())
    }

    pub fn list(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        let entries = match std::fs::read_dir(&self.templates_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(names),
            Err(e) => return Err(e),
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
                && let Some(stem) = p.file_stem()
            {
                names.push(stem.to_string_lossy().into_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn load(&self, name: &str) -> io::Result<MailContent> {
        Ok(MailContent::parse(&read_text(&self.path_of(name))?))
    }

    /// 另存範本（同名覆蓋）。回傳實際使用的名稱；名稱無效時回傳 None。
    pub fn save_as(&self, name: &str, content: &MailContent) -> io::Result<Option<String>> {
        let clean = sanitize_name(name);
        if clean.is_empty() {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.templates_dir)?;
        std::fs::write(self.path_of(&clean), content.serialize())?;
        Ok(Some(clean))
    }

    pub fn delete(&self, name: &str) -> io::Result<bool> {
        match std::fs::remove_file(self.path_of(name)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn path_of(&self, name: &str) -> PathBuf {
        self.templates_dir.join(format!("{name}.{EXTENSION}"))
    }
}

/// 將範本名稱中 Windows 檔名不允許的字元換成底線，並去除頭尾空白。
pub fn sanitize_name(name: &str) -> String {
    // 先去除頭尾空白再替換，避免頭尾的 Tab 被當成控制字元換成底線而殘留
    super::safe_file_name(name.trim_matches([' ', '\t'])).trim_matches([' ', '\t']).to_string()
}

pub fn read_text(path: &Path) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let store = TemplateStore::new(dir.path());

        assert_eq!(store.load_current().unwrap(), MailContent::default_content());
        store.ensure_templates_dir(&MailContent::default_content()).unwrap();
        assert_eq!(store.list().unwrap(), ["預設"]);

        let saved = store.save_as("  九月:特別版 ", &MailContent::new("S", "B")).unwrap();
        assert_eq!(saved.as_deref(), Some("九月_特別版"));
        assert_eq!(store.load("九月_特別版").unwrap(), MailContent::new("S", "B"));

        store.ensure_templates_dir(&MailContent::default_content()).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);

        assert!(store.delete("九月_特別版").unwrap());
        assert!(!store.delete("九月_特別版").unwrap());
        assert_eq!(store.save_as(" \t", &MailContent::new("S", "B")).unwrap(), None);
    }

    #[test]
    fn reads_cpp_text_mode_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = TemplateStore::new(dir.path());
        std::fs::write(dir.path().join("mailContent.default"), "主旨 {year}\r\nDear {employee_name},\r\n\r\n祝 順心").unwrap();
        let c = store.load_current().unwrap();
        assert_eq!(c.subject, "主旨 {year}");
        assert_eq!(c.body, "Dear {employee_name},\n\n祝 順心");
    }
}
