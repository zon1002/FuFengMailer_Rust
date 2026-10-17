use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use eframe::egui::{self, Align, Layout, RichText};
use egui_extras::{Column, TableBuilder};

use fufeng_mailer::core::employee::{Employee, status};
use fufeng_mailer::core::loader::{self, LoadResult};
use fufeng_mailer::core::mail_content::MailContent;
use fufeng_mailer::core::mailer::{self, BatchRequest, LettreTransport, MailJob, SendEvent, SmtpOptions};
use fufeng_mailer::core::settings::{AppSettings, MONTHS, SettingsStore, WindowBounds, YEARS};
use fufeng_mailer::core::templates::TemplateStore;

use super::editor::{EditorOutcome, MailEditor};
use super::style;

const MAX_LOG_LINES: usize = 200;

pub fn app_title() -> String {
    format!("福豐薪資單批量寄送工具 v{}", env!("CARGO_PKG_VERSION"))
}

/// 讀取設定；年份/月份預設為今天所在的年月。
pub fn initial_settings(store: &SettingsStore) -> AppSettings {
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    let defaults = AppSettings {
        year_idx: YEARS.iter().position(|y| *y == today.year().to_string()).unwrap_or(0),
        month_idx: today.month0() as usize,
        ..AppSettings::default()
    };
    store.load(defaults.clone()).unwrap_or(defaults)
}

/// 薪資單 PDF 或個人附件的管理視窗
#[derive(Clone, Copy, PartialEq)]
enum FileKind {
    Pdf,
    Personal,
}

struct Sending {
    rx: Receiver<SendEvent>,
    cancel: Arc<AtomicBool>,
    total: usize,
    done: usize,
}

pub struct MailerApp {
    s: AppSettings,
    settings_store: SettingsStore,
    templates: TemplateStore,
    mail_content: MailContent,
    employees: Vec<Employee>,
    log: VecDeque<String>,
    loading: Option<Receiver<LoadResult>>,
    sending: Option<Sending>,
    confirm_send: Option<usize>,
    file_manager: Option<(usize, FileKind)>,
    editor: Option<MailEditor>,
    /// 視窗「還原」狀態下的位置/大小，最大化時保留最大化前的值
    main_bounds: Option<WindowBounds>,
}

impl MailerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, data_dir: PathBuf, settings_store: SettingsStore, s: AppSettings) -> Self {
        style::install(&cc.egui_ctx);
        let templates = TemplateStore::new(&data_dir);
        let mut log = VecDeque::new();
        let mail_content = match templates.load_current() {
            Ok(c) => {
                if let Err(e) = templates.ensure_templates_dir(&c) {
                    log.push_back(stamp(&format!("[錯誤] 建立範本庫失敗: {e}")));
                }
                c
            }
            Err(e) => {
                log.push_back(stamp(&format!("[錯誤] 讀取郵件範本失敗: {e}")));
                MailContent::default_content()
            }
        };
        Self {
            main_bounds: s.main_window,
            s,
            settings_store,
            templates,
            mail_content,
            employees: Vec::new(),
            log,
            loading: None,
            sending: None,
            confirm_send: None,
            file_manager: None,
            editor: None,
        }
    }

    fn add_log(&mut self, msg: &str) {
        self.log.push_back(stamp(msg));
        while self.log.len() > MAX_LOG_LINES {
            self.log.pop_front();
        }
    }

    fn save_settings(&mut self) {
        self.s.main_window = self.main_bounds;
        if let Err(e) = self.settings_store.save(&self.s) {
            eprintln!("儲存設定失敗: {e}");
        }
    }

    /// 相對路徑（例如預設的 PDF 資料夾 PaySlips）以工作目錄為基準；未設定工作目錄時以 exe 所在目錄為基準。
    fn resolve(&self, path: &str) -> PathBuf {
        let path = path.trim();
        if path.is_empty() {
            return PathBuf::new();
        }
        let p = Path::new(path);
        if p.is_absolute() {
            return p.to_path_buf();
        }
        let work = Path::new(self.s.work_dir.trim());
        let base = if work.is_absolute() {
            work.to_path_buf()
        } else {
            std::env::current_exe().ok().and_then(|e| e.parent().map(Into::into)).unwrap_or_default()
        };
        base.join(p)
    }

    fn global_extra_available(&self) -> bool {
        !self.s.global_extra.trim().is_empty() && self.resolve(&self.s.global_extra).is_file()
    }

    fn refresh_all_statuses(&mut self) {
        if self.sending.is_some() {
            return;
        }
        let g = self.global_extra_available();
        self.employees.iter_mut().for_each(|e| e.refresh_pending_status(g));
    }

    // ──────────────────────────────────────────────────────────────
    //  背景工作
    // ──────────────────────────────────────────────────────────────
    fn start_load(&mut self, ctx: &egui::Context) {
        let xlsx = self.resolve(&self.s.xlsx_path);
        let pdf = self.resolve(&self.s.pdf_folder);
        if !xlsx.as_os_str().is_empty() {
            self.add_log(&format!("[..] 正在載入: {}", xlsx.display()));
        }
        let (year, month) = (self.s.year().to_string(), self.s.month().to_string());
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(loader::load(&xlsx, &pdf, &year, &month));
            ctx.request_repaint();
        });
        self.loading = Some(rx);
    }

    fn start_send(&mut self, ctx: &egui::Context) {
        let global: Vec<String> = if self.s.global_extra.trim().is_empty() {
            Vec::new()
        } else {
            vec![self.resolve(&self.s.global_extra).to_string_lossy().into_owned()]
        };
        let jobs: Vec<MailJob> = self
            .employees
            .iter()
            .enumerate()
            .filter(|(_, e)| e.checked)
            .map(|(i, e)| MailJob {
                index: i,
                emp_id: e.emp_id.clone(),
                name_key: e.name_key.clone(),
                email: e.email.clone(),
                pdf_files: e.pdf_files.clone(),
                extra_files: if e.attach_extra { global.iter().chain(&e.personal_files).cloned().collect() } else { Vec::new() },
            })
            .collect();
        let req = BatchRequest {
            smtp: SmtpOptions {
                host: self.s.smtp_server.trim().to_string(),
                port: self.s.smtp_port,
                account: self.s.email_account.trim().to_string(),
                password: self.s.password.clone(),
                skip_cert_validation: self.s.skip_cert_validation,
            },
            template: self.mail_content.clone(),
            year: self.s.year().into(),
            month: self.s.month().into(),
            pdf_folder: self.resolve(&self.s.pdf_folder),
            interval: Duration::from_secs(u64::from(self.s.interval_seconds.max(1))),
            jobs,
        };

        self.save_settings();
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.sending = Some(Sending { rx, cancel: cancel.clone(), total: req.jobs.len(), done: 0 });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let emit = |e: SendEvent| {
                let _ = tx.send(e);
                ctx.request_repaint();
            };
            mailer::send_batch(&req, &mut LettreTransport::default(), &cancel, &mailer::interruptible_wait, &emit);
        });
    }

    fn poll_background(&mut self) {
        if let Some(rx) = &self.loading
            && let Ok(result) = rx.try_recv()
        {
            self.loading = None;
            for m in &result.messages {
                self.add_log(m);
            }
            if result.success {
                self.employees = result.employees;
                self.refresh_all_statuses();
            }
        }

        let mut events = Vec::new();
        if let Some(s) = &self.sending {
            while let Ok(e) = s.rx.try_recv() {
                events.push(e);
            }
        }
        for e in events {
            match e {
                SendEvent::Status { index, status: text } => {
                    if text != status::SENDING
                        && let Some(s) = &mut self.sending
                    {
                        s.done += 1;
                    }
                    if let Some(emp) = self.employees.get_mut(index) {
                        emp.status = text;
                    }
                }
                SendEvent::Log(l) => self.add_log(&l),
                SendEvent::Finished { .. } => self.sending = None,
            }
        }
    }

    fn track_main_bounds(&mut self, ctx: &egui::Context) {
        let (outer, inner, maximized, minimized) =
            ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect, i.viewport().maximized, i.viewport().minimized));
        if minimized == Some(true) {
            return;
        }
        if maximized == Some(true) {
            if let Some(b) = &mut self.main_bounds {
                b.maximized = true;
            }
        } else if let (Some(o), Some(i)) = (outer, inner) {
            self.main_bounds = Some(WindowBounds { x: o.min.x, y: o.min.y, width: i.width(), height: i.height(), maximized: false });
        }
    }

    // ──────────────────────────────────────────────────────────────
    //  左欄：設定
    // ──────────────────────────────────────────────────────────────
    fn settings_panel(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let idle = self.sending.is_none();
        ui.add_space(4.0);
        style::section(ui, "基礎設定");
        ui.add_space(4.0);

        // 底部保留寄送按鈕（寄送中另加停止按鈕與進度條）的高度
        let reserve = if self.sending.is_some() { 140.0 } else { 70.0 };
        let scroll_height = (ui.available_height() - reserve).max(120.0);
        egui::ScrollArea::vertical().max_height(scroll_height).auto_shrink([false, false]).show(ui, |ui| {
            ui.add_enabled_ui(idle, |ui| {
                let width = ui.available_width() - 8.0;
                // 標籤與欄位同一列（同 C++ 版），節省垂直空間
                let field_w = width - 92.0;
                egui::Grid::new("smtp_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("SMTP 伺服器");
                    ui.add(egui::TextEdit::singleline(&mut self.s.smtp_server).hint_text("例如 smtp.gmail.com").desired_width(field_w));
                    ui.end_row();
                    ui.label("SMTP Port").on_hover_text("465 = SSL 直連；587 = STARTTLS");
                    ui.add(egui::DragValue::new(&mut self.s.smtp_port).range(1..=65535));
                    ui.end_row();
                    ui.label("寄件帳號");
                    ui.add(egui::TextEdit::singleline(&mut self.s.email_account).hint_text("例如 name@company.com").desired_width(field_w));
                    ui.end_row();
                    ui.label("帳號密碼");
                    ui.add(egui::TextEdit::singleline(&mut self.s.password).password(true).desired_width(field_w));
                    ui.end_row();
                });
                ui.checkbox(&mut self.s.remember_password, "記住密碼")
                    .on_hover_text("以 Windows 帳號加密儲存（DPAPI），只有本機目前的 Windows 使用者能解密");
                ui.checkbox(&mut self.s.skip_cert_validation, "略過 SSL 憑證驗證")
                    .on_hover_text("郵件伺服器使用自簽憑證時才需要勾選（與 C++ 版行為相同）");

                ui.separator();
                if let Some(dir) = path_row(ui, "工作目錄", &mut self.s.work_dir, "點右側 ... 選擇資料夾", width, || {
                    rfd::FileDialog::new().set_parent(frame).pick_folder()
                }) {
                    // 自動帶入 Excel 與 PDF 路徑
                    self.s.xlsx_path = dir.join("passwords.xlsx").to_string_lossy().into_owned();
                    self.s.pdf_folder = dir.join("PaySlips").to_string_lossy().into_owned();
                    self.add_log(&format!("[目錄] 工作目錄已設定: {}", dir.display()));
                }
                path_row(ui, "Excel 清單", &mut self.s.xlsx_path, "員工清單 .xlsx", width, || {
                    rfd::FileDialog::new().set_parent(frame).add_filter("Excel", &["xlsx", "xlsm", "xls"]).pick_file()
                });
                path_row(ui, "PDF 資料夾", &mut self.s.pdf_folder, "薪資單 PDF 所在資料夾", width, || {
                    rfd::FileDialog::new().set_parent(frame).pick_folder()
                });
                let before = self.s.global_extra.clone();
                path_row(ui, "全域附件", &mut self.s.global_extra, "勾選「附加」的員工會收到此附件", width, || {
                    rfd::FileDialog::new().set_parent(frame).pick_file()
                });
                if before != self.s.global_extra {
                    self.refresh_all_statuses();
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("年份");
                    combo(ui, "year", &mut self.s.year_idx, &YEARS, 64.0);
                    ui.label("月份");
                    combo(ui, "month", &mut self.s.month_idx, &MONTHS, 44.0);
                });
                ui.horizontal(|ui| {
                    ui.label("間隔(秒)");
                    ui.add(egui::DragValue::new(&mut self.s.interval_seconds).range(1..=3600));
                });

                ui.add_space(6.0);
                if ui.add_sized([width, 36.0], style::big_button("編輯郵件內容", 16.0)).clicked() {
                    self.editor = Some(MailEditor::new(&self.mail_content, &self.templates, self.s.editor_window));
                }
                let loading = self.loading.is_some();
                let label = if loading { "載入中…" } else { "1. 載入資料" };
                if ui.add_enabled(!loading, egui::Button::new(RichText::new(label).size(16.0)).min_size([width, 36.0].into())).clicked() {
                    self.start_load(ui.ctx());
                }
            });
        });

        // 寄送按鈕固定在左欄底部，不需捲動即可看到
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            let width = ui.available_width() - 8.0;
            ui.add_space(6.0);
            if let Some(s) = &self.sending {
                let cancelled = s.cancel.load(Ordering::Relaxed);
                let progress = s.done as f32 / s.total.max(1) as f32;
                ui.add(egui::ProgressBar::new(progress).desired_width(width).text(format!("{}/{}", s.done, s.total)));
                if ui.add_enabled(!cancelled, egui::Button::new("停止寄送").min_size([width, 32.0].into())).clicked() {
                    s.cancel.store(true, Ordering::Relaxed);
                    self.add_log("[郵件] 正在停止，目前這封處理完即停止…");
                }
            }
            let count = self.employees.iter().filter(|e| e.checked).count();
            if ui.add_enabled(idle && count > 0, style::send_button("2. 開始寄送").min_size([width, 48.0].into())).clicked() {
                if self.s.smtp_server.trim().is_empty() || self.s.email_account.trim().is_empty() {
                    self.add_log("[錯誤] 請填寫 SMTP 伺服器與寄件帳號");
                } else {
                    self.confirm_send = Some(count);
                }
            }
        });
    }

    // ──────────────────────────────────────────────────────────────
    //  員工清單
    // ──────────────────────────────────────────────────────────────
    fn employee_table(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            style::section(ui, "員工清單");
            if !self.employees.is_empty() {
                let checked = self.employees.iter().filter(|e| e.checked).count();
                ui.weak(format!("共 {} 筆，已勾選 {checked} 筆", self.employees.len()));
            }
        });

        let total = self.employees.len();
        let n_checked = self.employees.iter().filter(|e| e.checked).count();
        let n_attach = self.employees.iter().filter(|e| e.attach_extra).count();
        let mut toggle_all_checked = false;
        let mut toggle_all_attach = false;
        let mut changed = false;
        let mut open_manager = None;

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(100.0))
            .column(Column::initial(80.0).at_least(50.0))
            .column(Column::remainder().at_least(140.0).clip(true))
            .column(Column::exact(56.0))
            .column(Column::exact(64.0))
            .column(Column::exact(100.0))
            .column(Column::initial(170.0).at_least(120.0))
            .header(26.0, |mut h| {
                h.col(|ui| {
                    let mut all = total > 0 && n_checked == total;
                    let cb = egui::Checkbox::new(&mut all, RichText::new("工號").strong()).indeterminate(n_checked > 0 && n_checked < total);
                    toggle_all_checked = ui.add(cb).clicked();
                });
                h.col(|ui| _ = ui.strong("姓名"));
                h.col(|ui| _ = ui.strong("Email"));
                h.col(|ui| _ = ui.strong("薪單數"));
                h.col(|ui| {
                    let mut all = total > 0 && n_attach == total;
                    let cb = egui::Checkbox::new(&mut all, RichText::new("附加").strong()).indeterminate(n_attach > 0 && n_attach < total);
                    toggle_all_attach = ui.add(cb).clicked();
                });
                h.col(|ui| _ = ui.strong("個人附件"));
                h.col(|ui| _ = ui.strong("狀態"));
            })
            .body(|body| {
                body.rows(30.0, total, |mut row| {
                    let i = row.index();
                    let e = &mut self.employees[i];
                    row.col(|ui| changed |= ui.checkbox(&mut e.checked, &e.emp_id).changed());
                    row.col(|ui| _ = ui.label(&e.name_key));
                    row.col(|ui| _ = ui.label(&e.email).on_hover_text(&e.email));
                    row.col(|ui| {
                        if ui.button(e.pdf_files.len().to_string()).on_hover_text("管理薪資單 PDF").clicked() {
                            open_manager = Some((i, FileKind::Pdf));
                        }
                    });
                    row.col(|ui| changed |= ui.checkbox(&mut e.attach_extra, "").changed());
                    row.col(|ui| {
                        if ui.button(e.personal_button_text()).clicked() {
                            open_manager = Some((i, FileKind::Personal));
                        }
                    });
                    row.col(|ui| _ = ui.label(&e.status).on_hover_text(&e.status));
                });
            });

        if toggle_all_checked {
            let v = n_checked != total;
            self.employees.iter_mut().for_each(|e| e.checked = v);
            changed = true;
        }
        if toggle_all_attach {
            let v = n_attach != total;
            self.employees.iter_mut().for_each(|e| e.attach_extra = v);
            changed = true;
        }
        if changed {
            self.refresh_all_statuses();
        }
        if open_manager.is_some() {
            self.file_manager = open_manager;
        }
    }

    fn log_panel(&mut self, ui: &mut egui::Ui) {
        style::section(ui, "訊息記錄");
        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            for line in &self.log {
                ui.add(egui::Label::new(RichText::new(line).monospace()).selectable(true).wrap());
            }
        });
    }

    // ──────────────────────────────────────────────────────────────
    //  對話框
    // ──────────────────────────────────────────────────────────────
    fn confirm_send_modal(&mut self, ctx: &egui::Context) {
        let Some(count) = self.confirm_send else { return };
        let mut close = false;
        let mut go = false;
        egui::Modal::new(egui::Id::new("confirm_send")).show(ctx, |ui| {
            ui.heading("確認寄送");
            ui.add_space(8.0);
            ui.label(format!("準備寄送 {count} 封郵件，確認？"));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                go = ui.add(egui::Button::new("確定").min_size([90.0, 30.0].into())).clicked();
                close = ui.add(egui::Button::new("取消").min_size([90.0, 30.0].into())).clicked();
            });
        });
        if go {
            self.confirm_send = None;
            self.start_send(ctx);
        } else if close {
            self.confirm_send = None;
        }
    }

    fn file_manager_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some((idx, kind)) = self.file_manager else { return };
        let Some(emp) = self.employees.get(idx) else {
            self.file_manager = None;
            return;
        };
        let title = match kind {
            FileKind::Pdf => format!("薪資單管理 - {} ({})", emp.emp_id, emp.name_key),
            FileKind::Personal => format!("管理附件 - {} ({})", emp.emp_id, emp.name_key),
        };
        let mut open = true;
        let mut changed = false;
        let enabled = self.editor.is_none();
        egui::Window::new(title)
            .id(egui::Id::new("file_manager"))
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_size([500.0, 340.0])
            .show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let emp = &mut self.employees[idx];
                    let files = match kind {
                        FileKind::Pdf => &mut emp.pdf_files,
                        FileKind::Personal => &mut emp.personal_files,
                    };
                    ui.label(if kind == FileKind::Pdf { "目前 PDF 檔案：" } else { "目前附件：" });
                    let mut remove = None;
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(ui.available_height() - 50.0).auto_shrink([false, false]).show(ui, |ui| {
                            if files.is_empty() {
                                ui.weak("（無）");
                            }
                            for (i, f) in files.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui.small_button("刪除").clicked() {
                                        remove = Some(i);
                                    }
                                    let name = Path::new(f).file_name().map_or_else(|| f.clone(), |n| n.to_string_lossy().into_owned());
                                    ui.label(format!("• {name}")).on_hover_text(f);
                                });
                            }
                        });
                    });
                    if let Some(i) = remove {
                        files.remove(i);
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        let add_text = if kind == FileKind::Pdf { "新增 PDF" } else { "新增附件" };
                        if ui.button(add_text).clicked() {
                            let mut dlg = rfd::FileDialog::new().set_parent(frame);
                            if kind == FileKind::Pdf {
                                dlg = dlg.add_filter("PDF", &["pdf"]);
                            }
                            for p in dlg.pick_files().unwrap_or_default() {
                                let p = p.to_string_lossy().into_owned();
                                if !files.iter().any(|f| f.eq_ignore_ascii_case(&p)) {
                                    files.push(p);
                                    changed = true;
                                }
                            }
                        }
                        if kind == FileKind::Personal && ui.button("清空").clicked() {
                            files.clear();
                            changed = true;
                        }
                    });
                });
            });
        if changed {
            self.refresh_all_statuses();
        }
        if !open {
            self.file_manager = None;
        }
    }

    fn editor_viewport(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.editor else { return };
        let outcome = editor.show(ctx, &self.templates);
        let mut logs = std::mem::take(&mut editor.pending_logs);
        let bounds = editor.bounds;
        match outcome {
            EditorOutcome::Open => {}
            EditorOutcome::Cancelled => self.editor = None,
            EditorOutcome::Saved(content) => {
                self.editor = None;
                // 寫回 mailContent.default，僅影響本次及往後寄送內容，不動範本庫
                match self.templates.save_current(&content) {
                    Ok(()) => logs.push("[郵件] 已套用並儲存本次寄送內容".into()),
                    Err(e) => logs.push(format!("[錯誤] 儲存郵件內容失敗: {e}")),
                }
                self.mail_content = content;
            }
        }
        if bounds.is_some() {
            self.s.editor_window = bounds;
        }
        for l in logs {
            self.add_log(&l);
        }
    }
}

impl eframe::App for MailerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_background();
        self.track_main_bounds(&ctx);
        if self.loading.is_some() || self.sending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // 郵件範本編輯視窗開啟期間，主視窗暫停操作
        let enabled = self.editor.is_none();
        egui::Panel::left("settings").resizable(false).exact_size(320.0).show(ui, |ui| {
            ui.add_enabled_ui(enabled, |ui| self.settings_panel(ui, frame));
        });
        egui::Panel::bottom("log").resizable(true).default_size(190.0).show(ui, |ui| {
            ui.add_enabled_ui(enabled, |ui| self.log_panel(ui));
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_enabled_ui(enabled, |ui| self.employee_table(ui));
        });

        self.file_manager_window(&ctx, frame);
        self.confirm_send_modal(&ctx);
        self.editor_viewport(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(s) = &self.sending {
            s.cancel.store(true, Ordering::Relaxed);
        }
        self.save_settings();
    }
}

fn stamp(msg: &str) -> String {
    format!("[{}] {msg}", chrono::Local::now().format("%H:%M:%S"))
}

/// 標籤 + 文字框 + 「...」選擇按鈕；選擇後回傳所選路徑。
fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    width: f32,
    pick: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    ui.label(label);
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(width - 44.0));
        if ui.button(" ... ").clicked()
            && let Some(p) = pick()
        {
            *value = p.to_string_lossy().into_owned();
            picked = Some(p);
        }
    });
    picked
}

fn combo(ui: &mut egui::Ui, id: &str, idx: &mut usize, items: &[&str], width: f32) {
    egui::ComboBox::from_id_salt(id).width(width).selected_text(items[(*idx).min(items.len() - 1)]).show_ui(ui, |ui| {
        for (i, item) in items.iter().enumerate() {
            ui.selectable_value(idx, i, *item);
        }
    });
}
