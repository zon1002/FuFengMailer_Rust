# 福豐薪資單批量寄送工具 FuFengMailer（Rust 版）

**版本：v3.0.0**

Windows 桌面應用程式，讀取員工清單 Excel 與 PDF 薪資單，透過 SMTP 批量寄送薪資通知郵件，並支援附件管理與 EML 備份。

本版以 .NET 10 版（`C:\Fu_Project\FuFengMailer_NET10`，v2.0.0）為規格，改以 **Rust + egui** 重寫，功能與操作流程一致。

---

## 特色

- **單一執行檔**：約 8.5 MB，不需安裝 .NET、Visual C++ 可轉發套件或任何執行環境
- 資料存於獨立的 `%APPDATA%\FuFengMailer_Rust\`（`config.ini`、`mailContent.default`、`MailTemplates\`），**不沿用** C++ / .NET 版的設定，安裝後首次執行所有欄位皆為空白；檔案格式仍與其他版本相容，需要時可手動複製過來
- 密碼以 Windows DPAPI 加密（格式與 .NET 版相同）

---

## 功能

### 員工清單讀取
- 讀取 Excel 第一個工作表，第 1 列為標題，需含 `Emp`(工號)、`NameKey`(姓名)、`Email` 三欄（欄位順序不拘）
- 工號支援數字與字串；Email 不含 `@` 的列會略過並記錄；遇到第一個空白工號即視為清單結束
- Excel 正開著檔案時仍可讀取

### PDF 薪資單配對
- 命名格式：`{工號}-{年份}.{月份}*.pdf`（例如 `A001-2026.09_薪資單.pdf`）
- 有薪資單的員工預設勾選寄送；點「薪單數」按鈕可手動新增/刪除 PDF

### 批量郵件寄送
- Port 465 使用 SSL 直連；其他 Port（如 587）使用 STARTTLS
- 共用一條 SMTP 連線依序寄送，每封之間等待「間隔(秒)」
- 寄送前確認；寄送中可「停止寄送」並顯示進度
- 連線或登入失敗時中止整批

### 郵件內容與範本庫
- 範本變數：`{year}`、`{month}`、`{employee_name}`（主旨與本文皆可使用）
- 「編輯郵件內容」開啟**獨立視窗**，可移動、縮放、最大化；編輯期間主視窗暫停操作
- 「儲存」寫回 `mailContent.default`；範本庫可套用、另存、刪除（刪除前確認）
- 「載入預設」、「瀏覽模板檔...」

### 附件管理
- **全域附件**：套用於勾選「附加」的員工
- **個人附件**：每位員工可加入多個專屬附件
- 「工號」「附加」標題列勾選框可一鍵全選/全不選

### EML 備份
- 寄送成功後備份至 `{PDF資料夾}\Sent_Backups\{YYYYMM}\{工號}_{姓名}_備份.eml`

### 設定記憶（程式關閉時儲存）
- SMTP 伺服器/Port/帳號、略過憑證驗證、工作目錄、Excel 清單、PDF 資料夾、全域附件、年份、月份、間隔
- 主視窗與編輯視窗的位置/大小（鍵名 `RsMainWindow`、`RsEditorWindow`，與 .NET 版分開，因座標單位不同）
- 勾選「記住密碼」時以 DPAPI 加密儲存
- 安裝後首次執行所有欄位皆為空白（SMTP 伺服器、寄件帳號、密碼、工作目錄、Excel 清單、PDF 資料夾、全域附件）；選擇工作目錄時自動帶入 `passwords.xlsx` 與 `PaySlips`

---

## 操作步驟

1. 執行 `FuFengMailer.exe`
2. 填入 **SMTP 伺服器**、**Port**、**寄件帳號**、**帳號密碼**
3. 點「工作目錄」右側 `...` 選擇資料夾，自動帶入 `passwords.xlsx` 與 `PaySlips`
4. 設定**年份**、**月份**、**間隔(秒)**
5. 點「**1. 載入資料**」
6. （可選）全域附件、勾選「附加」、管理個人附件、「編輯郵件內容」
7. 點「**2. 開始寄送**」→ 確認

---

## 系統需求

- Windows 10 / 11，64 位元
- 顯示卡驅動需支援 OpenGL 2.0 以上（一般電腦皆內建）

---

## 建置說明（開發者）

| 工具 | 版本 |
|------|------|
| Rust | 1.85 以上（edition 2024），MSVC 工具鏈 |

```powershell
cargo test              # 單元測試
cargo run               # 開發執行（Debug 版會顯示主控台）
cargo build --release   # 發佈版：target\release\FuFengMailer.exe

# 產生安裝檔（需 Inno Setup 6，輸出 FuFengMailer_Rust_Setup_v{版本}.exe）
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" FuFengMailer_Setup.iss
```

> 安裝程式使用獨立的 AppId，安裝於 `Program Files\FuFengMailer_Rust`，可與 C++ / .NET 版並存。
> 設定檔與範本位於 `%APPDATA%\FuFengMailer_Rust`，解除安裝時保留。

### 專案結構

```
FuFengMailer_Rust/
├── Cargo.toml
├── build.rs                 ← 嵌入 exe 圖示與版本資訊
├── FuFengMailer_Setup.iss   ← Inno Setup 安裝程式腳本
├── assets/                  ← app.ico / app.png
└── src/
    ├── main.rs              ← 程式進入點、主視窗設定
    ├── lib.rs
    ├── core/                ← 商業邏輯（不依賴 GUI，含單元測試）
    │   ├── employee.rs      ← 員工資料、狀態文字
    │   ├── loader.rs        ← Excel 讀取（calamine）、PDF 配對
    │   ├── mailer.rs        ← SMTP 寄送（lettre）、EML 備份、停止
    │   ├── mail_content.rs  ← 範本變數、檔案格式
    │   ├── templates.rs     ← mailContent.default 與範本庫
    │   ├── settings.rs      ← config.ini 設定
    │   ├── ini.rs           ← INI 讀寫（相容 ANSI / UTF-16）
    │   └── dpapi.rs         ← 密碼加密
    └── ui/                  ← egui 介面
        ├── app.rs           ← 主視窗：設定、員工清單、訊息記錄、對話框
        ├── editor.rs        ← 編輯郵件範本視窗
        └── style.rs         ← 中文字型、樣式
```

### 技術架構

| 元件 | 技術 |
|------|------|
| GUI | egui / eframe 0.36（OpenGL） |
| Excel | calamine |
| SMTP | lettre（Windows SChannel TLS） |
| 檔案對話框 | rfd |
| 密碼加密 | Windows DPAPI（windows-sys） |
| 中文字型 | 微軟正黑體 → 微軟雅黑 → 黑體（自動 fallback） |
