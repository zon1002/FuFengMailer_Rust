# 修改紀錄 (Changelog)

所有版本依 [語義化版本](https://semver.org/lang/zh-TW/) 命名：`主版號.次版號.修訂號`

---

## [v3.0.0] — 2026-10-18

以 .NET 10 版 v2.0.0 為規格，改以 Rust + egui 全面重寫。

### 變更

- **技術架構**：.NET 10 / WinUI 3 → Rust / egui（eframe 0.36，OpenGL）；ClosedXML → calamine；MailKit → lettre
- **單一執行檔**（約 8.5 MB），不需 .NET 或 Windows App SDK 執行環境
- **資料目錄改為獨立的 `%APPDATA%\FuFengMailer_Rust`**，不沿用 C++ / .NET 版的設定與範本；安裝後首次執行所有欄位皆為空白（含 PDF 資料夾）
- 安裝程式使用獨立 AppId 與安裝目錄 `Program Files\FuFengMailer_Rust`，可與其他版本並存

### 沿用 .NET 版功能

- 員工清單載入、PDF 配對、全域/個人附件、EML 備份
- 郵件內容編輯（獨立可縮放視窗）與範本庫
- 465 SSL / 587 STARTTLS、略過 SSL 憑證驗證、停止寄送與進度條
- 程式關閉時記憶所有設定與視窗位置、DPAPI 加密記住密碼
