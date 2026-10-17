// Release 版不顯示主控台視窗
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui;

use fufeng_mailer::core::{default_data_dir, settings::SettingsStore};

fn main() -> eframe::Result {
    let data_dir = default_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let store = SettingsStore::new(&data_dir);
    let settings = ui::app::initial_settings(&store);

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(ui::app::app_title())
        .with_min_inner_size([960.0, 600.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/app.png")) {
        viewport = viewport.with_icon(icon);
    }
    // 還原上次的視窗位置/大小；沒有記錄時以 1200x760 置中
    let centered = match settings.main_window {
        Some(b) => {
            viewport = viewport
                .with_position([b.x, b.y])
                .with_inner_size([b.width, b.height])
                .with_maximized(b.maximized);
            false
        }
        None => {
            viewport = viewport.with_inner_size([1200.0, 760.0]);
            true
        }
    };

    let options = eframe::NativeOptions { viewport, centered, ..Default::default() };
    eframe::run_native(
        "FuFengMailer",
        options,
        Box::new(move |cc| Ok(Box::new(ui::app::MailerApp::new(cc, data_dir, store, settings)))),
    )
}
