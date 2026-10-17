// 將圖示與版本資訊嵌入 FuFengMailer.exe（檔案總管、工作列使用）
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("FileDescription", "福豐薪資單批量寄送工具");
        res.set("ProductName", "FuFengMailer");
        res.compile().expect("無法編譯 Windows 資源檔");
    }
}
