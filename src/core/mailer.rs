//! 批量寄送：共用一條 SMTP 連線依序寄出，每封之間等待指定秒數，成功後備份 EML。
//! 連線或登入失敗時中止整批，避免每位員工都等待間隔後重複失敗。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Message, SmtpTransport, Transport};

use super::employee::status;
use super::mail_content::{MailContent, normalize_newlines};

#[derive(Debug, Clone)]
pub struct SmtpOptions {
    pub host: String,
    pub port: u16,
    pub account: String,
    pub password: String,
    pub skip_cert_validation: bool,
}

/// 一封待寄郵件。`extra_files` 為全域附件與個人附件（已依「附加」勾選決定）。
#[derive(Debug, Clone)]
pub struct MailJob {
    pub index: usize,
    pub emp_id: String,
    pub name_key: String,
    pub email: String,
    pub pdf_files: Vec<String>,
    pub extra_files: Vec<String>,
}

pub struct BatchRequest {
    pub smtp: SmtpOptions,
    pub template: MailContent,
    pub year: String,
    pub month: String,
    pub pdf_folder: PathBuf,
    pub interval: Duration,
    pub jobs: Vec<MailJob>,
}

/// 寄送過程回報給 UI 的事件。
#[derive(Debug, Clone, PartialEq)]
pub enum SendEvent {
    Status { index: usize, status: String },
    Log(String),
    Finished { sent: usize },
}

/// SMTP 傳輸抽象，方便以假物件測試寄送流程。
pub trait MailTransport {
    fn is_connected(&self) -> bool;
    fn connect(&mut self, options: &SmtpOptions) -> Result<(), String>;
    fn send(&mut self, message: &Message) -> Result<(), String>;
}

/// 以 lettre 實作的 SMTP 傳輸。Port 465 走 SSL 直連，其餘 Port（如 587）使用 STARTTLS。
#[derive(Default)]
pub struct LettreTransport {
    inner: Option<SmtpTransport>,
}

impl MailTransport for LettreTransport {
    fn is_connected(&self) -> bool {
        self.inner.is_some()
    }

    fn connect(&mut self, o: &SmtpOptions) -> Result<(), String> {
        let tls = TlsParameters::builder(o.host.clone())
            .dangerous_accept_invalid_certs(o.skip_cert_validation)
            .dangerous_accept_invalid_hostnames(o.skip_cert_validation)
            .build()
            .map_err(|e| e.to_string())?;
        let mut builder = SmtpTransport::builder_dangerous(&o.host)
            .port(o.port)
            .tls(if o.port == 465 { Tls::Wrapper(tls) } else { Tls::Opportunistic(tls) })
            .timeout(Some(Duration::from_secs(60)));
        if !o.password.is_empty() {
            builder = builder.credentials(Credentials::new(o.account.clone(), o.password.clone()));
        }
        let transport = builder.build();
        // 先實際連線並登入一次，連線/帳密錯誤時立即中止整批
        match transport.test_connection() {
            Ok(true) => {
                self.inner = Some(transport);
                Ok(())
            }
            Ok(false) => Err("伺服器沒有回應".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn send(&mut self, message: &Message) -> Result<(), String> {
        let t = self.inner.as_ref().ok_or("尚未連線")?;
        t.send(message).map(|_| ()).map_err(|e| e.to_string())
    }
}

/// 建立郵件：純文字 UTF-8 本文，附件中不存在的檔案自動略過（與 C++ 版相同）。
pub fn build_message(from: &str, to: &str, subject: &str, body: &str, attachments: &[String]) -> Result<Message, String> {
    let from: Mailbox = from.parse().map_err(|e| format!("寄件帳號格式錯誤: {e}"))?;
    let to: Mailbox = to.parse().map_err(|e| format!("收件地址格式錯誤: {e}"))?;
    let mut multipart = MultiPart::mixed().singlepart(SinglePart::plain(normalize_newlines(body)));
    for path in attachments {
        let p = Path::new(path);
        let Ok(data) = std::fs::read(p) else { continue };
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ct = if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            ContentType::parse("application/pdf")
        } else {
            ContentType::parse("application/octet-stream")
        }
        .map_err(|e| e.to_string())?;
        multipart = multipart.singlepart(Attachment::new(name).body(data, ct));
    }
    Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .multipart(multipart)
        .map_err(|e| e.to_string())
}

/// {PDF資料夾}\Sent_Backups\{YYYYMM}\{工號}_{姓名}_備份.eml
pub fn backup_path(pdf_folder: &Path, year: &str, month: &str, emp_id: &str, name: &str) -> PathBuf {
    pdf_folder
        .join("Sent_Backups")
        .join(format!("{year}{month}"))
        .join(super::safe_file_name(&format!("{emp_id}_{name}_備份.eml")))
}

/// 依序寄出所有郵件，回傳成功封數。`wait` 回傳 false 表示等待期間被要求停止。
pub fn send_batch(
    req: &BatchRequest,
    transport: &mut dyn MailTransport,
    cancel: &AtomicBool,
    wait: &dyn Fn(Duration, &AtomicBool) -> bool,
    emit: &dyn Fn(SendEvent),
) -> usize {
    let mut sent = 0;
    let status = |index, s: &str| emit(SendEvent::Status { index, status: s.to_string() });
    let log = |s: String| emit(SendEvent::Log(s));

    for (i, job) in req.jobs.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        status(job.index, status::SENDING);

        if !transport.is_connected()
            && let Err(e) = transport.connect(&req.smtp)
        {
            status(job.index, status::FAILED);
            log(format!("[錯誤] 無法連線/登入 SMTP 伺服器 {}:{} - {e}，已中止寄送", req.smtp.host, req.smtp.port));
            break;
        }

        let extras: Vec<String> = job.extra_files.iter().filter(|f| Path::new(f).is_file()).cloned().collect();
        let content = req.template.render(&req.year, &req.month, &job.name_key);
        let attachments: Vec<String> = job.pdf_files.iter().chain(&extras).cloned().collect();
        let result = build_message(&req.smtp.account, &job.email, &content.subject, &content.body, &attachments)
            .and_then(|m| transport.send(&m).map(|_| m));

        match result {
            Ok(message) => {
                sent += 1;
                status(job.index, &status::sent(job.pdf_files.len(), extras.len()));
                log(format!("[OK] 成功: {} ({}個PDF + {}個附件)", job.name_key, job.pdf_files.len(), extras.len()));
                let path = backup_path(&req.pdf_folder, &req.year, &req.month, &job.emp_id, &job.name_key);
                let saved = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&path, message.formatted()));
                if let Err(e) = saved {
                    log(format!("[!] {} 備份 EML 失敗: {e}", job.name_key));
                }
            }
            Err(e) => {
                status(job.index, status::FAILED);
                log(format!("[錯誤] 失敗: {} - {e}", job.name_key));
            }
        }

        if i + 1 < req.jobs.len() && !wait(req.interval, cancel) {
            break;
        }
    }

    log(if cancel.load(Ordering::Relaxed) {
        format!("[郵件] 已停止，已寄出 {sent} 封。")
    } else {
        format!("[郵件] 完成，已寄出 {sent} 封。")
    });
    emit(SendEvent::Finished { sent });
    sent
}

/// 每 100ms 檢查一次是否被要求停止的等待；回傳 false 表示已被停止。
pub fn interruptible_wait(total: Duration, cancel: &AtomicBool) -> bool {
    let step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited < total {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(step.min(total - waited));
        waited += step;
    }
    !cancel.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeTransport {
        sent: Vec<Message>,
        connects: usize,
        connected: bool,
        connect_error: Option<String>,
        fail_to: Option<String>,
    }

    impl MailTransport for FakeTransport {
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn connect(&mut self, _: &SmtpOptions) -> Result<(), String> {
            self.connects += 1;
            if let Some(e) = &self.connect_error {
                return Err(e.clone());
            }
            self.connected = true;
            Ok(())
        }
        fn send(&mut self, m: &Message) -> Result<(), String> {
            let to = m.envelope().to()[0].to_string();
            if self.fail_to.as_deref() == Some(to.as_str()) {
                return Err("550 rejected".into());
            }
            self.sent.push(m.clone());
            Ok(())
        }
    }

    fn job(index: usize, id: &str, name: &str, email: &str) -> MailJob {
        MailJob { index, emp_id: id.into(), name_key: name.into(), email: email.into(), pdf_files: vec![], extra_files: vec![] }
    }

    fn request(dir: &Path, jobs: Vec<MailJob>) -> BatchRequest {
        BatchRequest {
            smtp: SmtpOptions { host: "smtp.test".into(), port: 465, account: "hr@example.com".into(), password: "pw".into(), skip_cert_validation: false },
            template: MailContent::new("[薪資] {year}年{month}月", "Dear {employee_name}"),
            year: "2026".into(),
            month: "09".into(),
            pdf_folder: dir.to_path_buf(),
            interval: Duration::from_secs(15),
            jobs,
        }
    }

    fn run(req: &BatchRequest, t: &mut FakeTransport, cancel: &AtomicBool, wait: &dyn Fn(Duration, &AtomicBool) -> bool) -> (usize, Vec<SendEvent>) {
        let events = RefCell::new(Vec::new());
        let sent = send_batch(req, t, cancel, wait, &|e| events.borrow_mut().push(e));
        (sent, events.into_inner())
    }

    fn logs(events: &[SendEvent]) -> Vec<String> {
        events.iter().filter_map(|e| if let SendEvent::Log(l) = e { Some(l.clone()) } else { None }).collect()
    }

    #[test]
    fn sends_with_attachments_backup_and_interval() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("1001-2026.09.pdf");
        let extra = dir.path().join("公告.txt");
        std::fs::write(&pdf, "%PDF").unwrap();
        std::fs::write(&extra, "notice").unwrap();

        let mut j1 = job(0, "1001", "王小明", "ming@example.com");
        j1.pdf_files = vec![pdf.to_string_lossy().into()];
        j1.extra_files = vec![extra.to_string_lossy().into(), "Z:\\missing.txt".into()];
        let req = request(dir.path(), vec![j1, job(3, "1002", "陳大文", "chen@example.com")]);

        let waits = RefCell::new(Vec::new());
        let mut t = FakeTransport::default();
        let (sent, events) = run(&req, &mut t, &AtomicBool::new(false), &|d, _| {
            waits.borrow_mut().push(d);
            true
        });

        assert_eq!(sent, 2);
        assert_eq!(t.connects, 1);
        assert_eq!(*waits.borrow(), [Duration::from_secs(15)]); // 最後一封後不等待

        let raw = String::from_utf8(t.sent[0].formatted()).unwrap();
        assert!(raw.contains("Content-Type: application/pdf"));
        assert!(raw.contains("1001-2026.09.pdf"));
        assert!(!raw.contains("missing.txt"));

        let statuses: Vec<_> = events.iter().filter(|e| matches!(e, SendEvent::Status { .. })).cloned().collect();
        assert_eq!(
            statuses,
            [
                SendEvent::Status { index: 0, status: status::SENDING.into() },
                SendEvent::Status { index: 0, status: "[OK] 1PDF+1附件".into() },
                SendEvent::Status { index: 3, status: status::SENDING.into() },
                SendEvent::Status { index: 3, status: "[OK] 0PDF+0附件".into() },
            ]
        );
        assert!(backup_path(dir.path(), "2026", "09", "1001", "王小明").is_file());
        assert_eq!(logs(&events).last().unwrap(), "[郵件] 完成，已寄出 2 封。");
        assert_eq!(events.last(), Some(&SendEvent::Finished { sent: 2 }));
    }

    #[test]
    fn subject_and_body_are_rendered() {
        let m = build_message("hr@example.com", "a@example.com", "[薪資] 2026年09月", "Dear 王小明", &[]).unwrap();
        let raw = String::from_utf8(m.formatted()).unwrap();
        assert!(raw.contains("Subject: =?utf-8?"), "{raw}");
    }

    #[test]
    fn failed_send_continues() {
        let dir = tempfile::tempdir().unwrap();
        let req = request(dir.path(), vec![job(0, "1", "甲", "bad@example.com"), job(1, "2", "乙", "ok@example.com")]);
        let mut t = FakeTransport { fail_to: Some("bad@example.com".into()), ..Default::default() };
        let (sent, events) = run(&req, &mut t, &AtomicBool::new(false), &|_, _| true);
        assert_eq!(sent, 1);
        assert!(events.contains(&SendEvent::Status { index: 0, status: status::FAILED.into() }));
        assert!(logs(&events).contains(&"[錯誤] 失敗: 甲 - 550 rejected".to_string()));
    }

    #[test]
    fn connection_failure_aborts_batch() {
        let dir = tempfile::tempdir().unwrap();
        let req = request(dir.path(), vec![job(0, "1", "甲", "a@example.com"), job(1, "2", "乙", "b@example.com")]);
        let mut t = FakeTransport { connect_error: Some("auth failed".into()), ..Default::default() };
        let (sent, events) = run(&req, &mut t, &AtomicBool::new(false), &|_, _| true);
        assert_eq!(sent, 0);
        assert_eq!(t.connects, 1);
        assert!(!events.iter().any(|e| matches!(e, SendEvent::Status { index: 1, .. })));
        assert!(logs(&events).iter().any(|l| l.contains("auth failed") && l.contains("已中止寄送")));
    }

    #[test]
    fn cancel_during_interval_stops() {
        let dir = tempfile::tempdir().unwrap();
        let req = request(dir.path(), vec![job(0, "1", "甲", "a@example.com"), job(1, "2", "乙", "b@example.com")]);
        let cancel = AtomicBool::new(false);
        let mut t = FakeTransport::default();
        let (sent, events) = run(&req, &mut t, &cancel, &|_, c| {
            c.store(true, Ordering::Relaxed);
            false
        });
        assert_eq!(sent, 1);
        assert_eq!(logs(&events).last().unwrap(), "[郵件] 已停止，已寄出 1 封。");
    }

    #[test]
    fn interruptible_wait_returns_early() {
        let cancel = AtomicBool::new(true);
        assert!(!interruptible_wait(Duration::from_secs(60), &cancel));
        assert!(interruptible_wait(Duration::from_millis(10), &AtomicBool::new(false)));
    }

    #[test]
    fn backup_path_replaces_invalid_chars() {
        let p = backup_path(Path::new("C:\\pdf"), "2026", "09", "A/1", "王:小明");
        assert_eq!(p, Path::new("C:\\pdf\\Sent_Backups\\202609\\A_1_王_小明_備份.eml"));
    }
}
