//! 編輯本次寄送內容，並管理範本庫（套用/另存/刪除）。
//! 套用範本後仍可自由修改，按「儲存」只影響寄送內容，不會覆蓋範本庫中的原始範本。
//! 以獨立的作業系統視窗（egui viewport）呈現，可自由移動、縮放、最大化。

use eframe::egui::{self, Layout, RichText, ViewportBuilder, ViewportId};

use fufeng_mailer::core::mail_content::{MailContent, normalize_newlines};
use fufeng_mailer::core::settings::WindowBounds;
use fufeng_mailer::core::templates::{TemplateStore, read_text};

use super::style;

pub enum EditorOutcome {
    Open,
    Cancelled,
    Saved(MailContent),
}

pub struct MailEditor {
    subject: String,
    body: String,
    names: Vec<String>,
    selected: Option<usize>,
    new_name: String,
    confirm_delete: bool,
    builder: ViewportBuilder,
    /// 目前（還原狀態下）的視窗位置/大小，關閉後由主視窗存入 config.ini
    pub bounds: Option<WindowBounds>,
    /// 尚未寫入主視窗訊息記錄的訊息
    pub pending_logs: Vec<String>,
}

impl MailEditor {
    pub fn new(current: &MailContent, store: &TemplateStore, saved: Option<WindowBounds>) -> Self {
        let mut builder = ViewportBuilder::default()
            .with_title("編輯郵件範本")
            .with_min_inner_size([560.0, 520.0])
            .with_minimize_button(false);
        if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/app.png")) {
            builder = builder.with_icon(icon);
        }
        // 還原上次的位置/大小；沒有記錄時預設 720x680
        builder = match saved {
            Some(b) => builder.with_position([b.x, b.y]).with_inner_size([b.width, b.height]).with_maximized(b.maximized),
            None => builder.with_inner_size([720.0, 680.0]),
        };
        let mut me = Self {
            subject: current.subject.clone(),
            body: current.body.clone(),
            names: Vec::new(),
            selected: None,
            new_name: String::new(),
            confirm_delete: false,
            builder,
            bounds: saved,
            pending_logs: Vec::new(),
        };
        me.reload(store, None);
        me
    }

    fn content(&self) -> MailContent {
        MailContent::new(self.subject.clone(), normalize_newlines(&self.body))
    }

    fn reload(&mut self, store: &TemplateStore, select: Option<&str>) {
        match store.list() {
            Ok(names) => self.names = names,
            Err(e) => self.pending_logs.push(format!("[錯誤] 讀取範本庫失敗: {e}")),
        }
        self.selected = select.and_then(|s| self.names.iter().position(|n| n == s));
        self.confirm_delete = false;
    }

    pub fn show(&mut self, ctx: &egui::Context, store: &TemplateStore) -> EditorOutcome {
        let builder = self.builder.clone();
        ctx.show_viewport_immediate(ViewportId::from_hash_of("mail_editor"), builder, |ui, _class| {
            self.track_bounds(ui.ctx());
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                return EditorOutcome::Cancelled;
            }
            let mut outcome = EditorOutcome::Open;
            egui::Panel::bottom("editor_bottom").show(ui, |ui| outcome = self.library_and_buttons(ui, store));
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("郵件主旨");
                ui.add(egui::TextEdit::singleline(&mut self.subject).desired_width(f32::INFINITY));
                ui.add_space(4.0);
                ui.label("郵件本文");
                // 本文框佔滿剩餘高度，隨視窗縮放
                let height = (ui.available_height() - 28.0).max(80.0);
                egui::ScrollArea::vertical().max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), height],
                        egui::TextEdit::multiline(&mut self.body).desired_width(f32::INFINITY),
                    );
                });
                ui.weak("可用變數：{year} 年份、{month} 月份、{employee_name} 員工姓名");
            });
            outcome
        })
    }

    fn track_bounds(&mut self, ctx: &egui::Context) {
        let (outer, inner, maximized) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect, i.viewport().maximized));
        if maximized == Some(true) {
            if let Some(b) = &mut self.bounds {
                b.maximized = true;
            }
        } else if let (Some(o), Some(i)) = (outer, inner) {
            self.bounds = Some(WindowBounds { x: o.min.x, y: o.min.y, width: i.width(), height: i.height(), maximized: false });
        }
    }

    fn library_and_buttons(&mut self, ui: &mut egui::Ui, store: &TemplateStore) -> EditorOutcome {
        ui.add_space(6.0);
        style::section(ui, "範本庫");
        ui.horizontal(|ui| {
            let text = self.selected.map_or("(尚未選擇)", |i| self.names[i].as_str()).to_string();
            egui::ComboBox::from_id_salt("template").width(240.0).selected_text(text).show_ui(ui, |ui| {
                for (i, n) in self.names.iter().enumerate() {
                    if ui.selectable_label(self.selected == Some(i), n).clicked() {
                        self.selected = Some(i);
                        self.confirm_delete = false;
                    }
                }
            });
            let has = self.selected.is_some();
            if ui.add_enabled(has, egui::Button::new("套用")).clicked()
                && let Some(name) = self.selected.map(|i| self.names[i].clone())
            {
                match store.load(&name) {
                    Ok(c) => {
                        self.subject = c.subject;
                        self.body = c.body;
                        self.pending_logs.push(format!("[範本] 已套用範本: {name}"));
                    }
                    Err(e) => {
                        self.pending_logs.push(format!("[錯誤] 找不到範本: {name} - {e}"));
                        self.reload(store, None);
                    }
                }
            }
            if ui.add_enabled(has, egui::Button::new("刪除範本")).clicked() {
                self.confirm_delete = true;
            }
        });

        // 刪除前確認
        if self.confirm_delete
            && let Some(name) = self.selected.map(|i| self.names[i].clone())
        {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("確定要刪除範本「{name}」嗎？")).color(ui.visuals().warn_fg_color));
                if ui.button("確定刪除").clicked() {
                    match store.delete(&name) {
                        Ok(true) => self.pending_logs.push(format!("[範本] 已刪除範本: {name}")),
                        Ok(false) => self.pending_logs.push(format!("[錯誤] 刪除範本失敗: {name}")),
                        Err(e) => self.pending_logs.push(format!("[錯誤] 刪除範本失敗: {name} - {e}")),
                    }
                    self.reload(store, None);
                }
                if ui.button("取消").clicked() {
                    self.confirm_delete = false;
                }
            });
        }

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("新範本名稱").desired_width(240.0));
            if ui.button("另存為範本").clicked() {
                match store.save_as(&self.new_name, &self.content()) {
                    Ok(Some(saved)) => {
                        self.pending_logs.push(format!("[範本] 已儲存範本: {saved}"));
                        self.new_name.clear();
                        self.reload(store, Some(&saved));
                    }
                    Ok(None) => self.pending_logs.push("[錯誤] 範本名稱無效".into()),
                    Err(e) => self.pending_logs.push(format!("[錯誤] 儲存範本失敗: {e}")),
                }
            }
        });

        ui.separator();
        let mut outcome = EditorOutcome::Open;
        ui.horizontal(|ui| {
            if ui.button("載入預設").clicked() {
                let d = MailContent::default_content();
                self.subject = d.subject;
                self.body = d.body;
            }
            if ui.button("瀏覽模板檔...").clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("模板檔案", &["default", "template", "txt"]).pick_file()
            {
                match read_text(&path) {
                    Ok(text) => {
                        let c = MailContent::parse(&text);
                        // 與 C++ 版相同：空白的主旨/本文不覆蓋目前內容
                        if !c.subject.is_empty() {
                            self.subject = c.subject;
                        }
                        if !c.body.is_empty() {
                            self.body = c.body;
                        }
                        self.pending_logs.push(format!("[模板] 已載入: {}", path.display()));
                    }
                    Err(e) => self.pending_logs.push(format!("[錯誤] 無法開啟模板檔: {} - {e}", path.display())),
                }
            }
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("取消").min_size([90.0, 30.0].into())).clicked() {
                    outcome = EditorOutcome::Cancelled;
                }
                if ui.add(style::big_button("儲存", 15.0).min_size([90.0, 30.0].into())).clicked() {
                    outcome = EditorOutcome::Saved(self.content());
                }
            });
        });
        ui.add_space(6.0);
        outcome
    }
}
