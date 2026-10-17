/// 員工清單中的一筆資料。
#[derive(Debug, Clone, PartialEq)]
pub struct Employee {
    pub emp_id: String,
    pub name_key: String,
    pub email: String,
    /// 找到的薪資單 PDF 完整路徑
    pub pdf_files: Vec<String>,
    /// 個人附件完整路徑
    pub personal_files: Vec<String>,
    /// 是否寄送
    pub checked: bool,
    /// 是否附加全域附件與個人附件
    pub attach_extra: bool,
    pub status: String,
}

impl Employee {
    pub fn new(emp_id: impl Into<String>, name_key: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            emp_id: emp_id.into(),
            name_key: name_key.into(),
            email: email.into(),
            pdf_files: Vec::new(),
            personal_files: Vec::new(),
            checked: false,
            attach_extra: false,
            status: String::new(),
        }
    }

    /// 額外附件數（全域附件 + 個人附件），只在勾選「附加」時計入。
    pub fn attachment_count(&self, global_extra_available: bool) -> usize {
        if !self.attach_extra {
            return 0;
        }
        usize::from(global_extra_available) + self.personal_files.len()
    }

    /// 依目前勾選/附件狀態重算「待寄」狀態文字。
    pub fn refresh_pending_status(&mut self, global_extra_available: bool) {
        self.status = status::pending(self.checked, self.pdf_files.len(), self.attachment_count(global_extra_available));
    }

    pub fn personal_button_text(&self) -> String {
        if self.personal_files.is_empty() {
            "管理附件".into()
        } else {
            format!("管理({}個)", self.personal_files.len())
        }
    }
}

/// 員工清單「狀態」欄的顯示文字，與 C++ / .NET 版一致。
pub mod status {
    pub const SENDING: &str = ">>> 寄送中";
    pub const FAILED: &str = "[X] 失敗";
    pub const SKIPPED: &str = "[X] 不寄送";

    pub fn pending(checked: bool, pdf_count: usize, attach_count: usize) -> String {
        if !checked {
            return SKIPPED.into();
        }
        let mut parts = Vec::new();
        if pdf_count > 0 {
            parts.push(format!("{pdf_count}個PDF"));
        }
        if attach_count > 0 {
            parts.push(format!("{attach_count}個附件"));
        }
        if parts.is_empty() {
            "無薪資單，待寄".into()
        } else {
            format!("{}，待寄", parts.join(" + "))
        }
    }

    pub fn sent(pdf_count: usize, attach_count: usize) -> String {
        format!("[OK] {pdf_count}PDF+{attach_count}附件")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_status_matches_previous_versions() {
        assert_eq!(status::pending(false, 2, 1), "[X] 不寄送");
        assert_eq!(status::pending(true, 0, 0), "無薪資單，待寄");
        assert_eq!(status::pending(true, 2, 0), "2個PDF，待寄");
        assert_eq!(status::pending(true, 0, 3), "3個附件，待寄");
        assert_eq!(status::pending(true, 1, 2), "1個PDF + 2個附件，待寄");
    }

    #[test]
    fn attachment_count_only_when_attach_extra() {
        let mut e = Employee::new("A001", "王小明", "a@b.c");
        e.personal_files.push("p1".into());
        assert_eq!(e.attachment_count(true), 0);
        e.attach_extra = true;
        assert_eq!(e.attachment_count(true), 2);
        assert_eq!(e.attachment_count(false), 1);
        assert_eq!(e.personal_button_text(), "管理(1個)");
    }
}
