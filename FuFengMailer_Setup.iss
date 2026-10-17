; ---------------------------------------------------------------
; 福豐薪資單批量寄送工具 v3.0.0（Rust / egui）- Inno Setup 安裝程式腳本
;
; 程式為單一執行檔（圖示與資源已內嵌），不需任何執行環境。
;
; 產生安裝檔（於此檔案所在目錄執行）：
;   cargo build --release
;   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" FuFengMailer_Setup.iss
; 輸出：FuFengMailer_Rust_Setup_v{版本}.exe
; 版本號需與 Cargo.toml 的 version 一致。
; ---------------------------------------------------------------

#define AppName "FuFengMailer_Rust"
#define AppDisplay "福豐薪資單批量寄送工具 (Rust 版)"
#define AppVersion "3.0.0"
#define AppPublisher "福豐自動化"
#define AppExe "FuFengMailer.exe"
#define BuildDir "target\release"

[Setup]
; 獨立的 AppId 與安裝目錄：可與 C++ / .NET 版並存，不會互相覆蓋。
; 若日後要改為升級取代 .NET 版，改用 .NET 版的 AppId 與 DefaultDirName 即可。
AppId={{577CD2E2-EFC4-47A7-B5E0-B8CAC98EFD15}
AppName={#AppDisplay}
AppVersion={#AppVersion}
AppVerName={#AppDisplay} v{#AppVersion}
AppPublisher={#AppPublisher}
VersionInfoVersion={#AppVersion}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppDisplay}
DisableProgramGroupPage=yes
OutputBaseFilename=FuFengMailer_Rust_Setup_v{#AppVersion}
OutputDir=.
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=admin
SetupIconFile=assets\app.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppDisplay}
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "ChineseTraditional"; MessagesFile: "compiler:Languages\ChineseTraditional.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#BuildDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppDisplay}"; Filename: "{app}\{#AppExe}"
Name: "{group}\解除安裝 {#AppDisplay}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppDisplay}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "立即啟動 {#AppDisplay}"; Flags: nowait postinstall skipifsilent

; 設定檔與郵件範本位於 %APPDATA%\FuFengMailer（與 C++ / .NET 版共用），EML 備份位於
; 使用者的 PDF 資料夾下 Sent_Backups\，解除安裝時刻意保留。
