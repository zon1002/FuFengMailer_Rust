use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

/// 中文字型優先順序：微軟正黑體 → 微軟雅黑 → 黑體（同 C++ 版）
const CJK_FONTS: [&str; 3] = ["C:\\Windows\\Fonts\\msjh.ttc", "C:\\Windows\\Fonts\\msyh.ttc", "C:\\Windows\\Fonts\\simhei.ttf"];

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(bytes) = CJK_FONTS.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("cjk".into(), Arc::new(FontData::from_owned(bytes)));
        // 正黑體本身含英數字，放第一順位讓中英文字形一致；等寬字型則作為後備
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "cjk".into());
        fonts.families.entry(FontFamily::Monospace).or_default().push("cjk".into());
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(20.0)),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
    });
}

/// 區塊標題（藍色）
pub fn section(ui: &mut egui::Ui, text: &str) {
    let color = if ui.visuals().dark_mode { egui::Color32::from_rgb(110, 180, 255) } else { egui::Color32::from_rgb(0, 95, 184) };
    ui.label(egui::RichText::new(text).heading().color(color));
}

/// 加大的主要按鈕
pub fn big_button(text: &str, size: f32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_string()).size(size))
}

/// 綠色「開始寄送」按鈕
pub fn send_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_string()).size(20.0).strong().color(egui::Color32::WHITE))
        .fill(egui::Color32::from_rgb(33, 150, 33))
}
