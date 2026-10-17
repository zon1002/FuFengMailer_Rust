/// 郵件主旨與本文範本，支援 {year}、{month}、{employee_name} 變數。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailContent {
    pub subject: String,
    pub body: String,
}

impl MailContent {
    pub fn new(subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self { subject: subject.into(), body: body.into() }
    }

    pub fn default_content() -> Self {
        Self::new(
            "[薪資通知] 您的 {year}年{month}月份 薪資單",
            "Dear {employee_name},\n\n您好！附件為您 {year}年{month}月份 的電子薪資單。檔案已加密，密碼為您的 [身分證字號]。\n\n祝 順心\n人事部 敬上",
        )
    }

    pub fn render(&self, year: &str, month: &str, employee_name: &str) -> Self {
        let apply = |t: &str| {
            t.replace("{employee_name}", employee_name)
                .replace("{year}", year)
                .replace("{month}", month)
        };
        Self::new(apply(&self.subject), apply(&self.body))
    }

    /// 範本檔格式：第一行為主旨，其餘為本文（與 C++ / .NET 版相容）。
    pub fn parse(file_text: &str) -> Self {
        let text = file_text.trim_start_matches('\u{FEFF}').replace("\r\n", "\n");
        match text.split_once('\n') {
            Some((subject, body)) => Self::new(subject, body),
            None => Self::new(text, ""),
        }
    }

    pub fn serialize(&self) -> String {
        format!("{}\n{}", self.subject, normalize_newlines(&self.body))
    }
}

/// CRLF / CR 一律轉為 LF。
pub fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_replaces_all_variables() {
        let c = MailContent::new("{year}/{month} {employee_name}", "Hi {employee_name}, {year}年{month}月");
        let r = c.render("2026", "09", "王小明");
        assert_eq!(r.subject, "2026/09 王小明");
        assert_eq!(r.body, "Hi 王小明, 2026年09月");
    }

    #[test]
    fn parse_splits_subject_and_normalizes_crlf() {
        let c = MailContent::parse("\u{FEFF}主旨\r\n第一行\r\n第二行");
        assert_eq!(c.subject, "主旨");
        assert_eq!(c.body, "第一行\n第二行");
        assert_eq!(MailContent::parse(&c.serialize()), c);
        assert_eq!(MailContent::parse("只有主旨"), MailContent::new("只有主旨", ""));
    }
}
