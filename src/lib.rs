//! 福豐薪資單批量寄送工具的商業邏輯（不依賴 GUI）。
//!
//! 以 .NET 10 版（FuFengMailer_NET10）為參考規格。資料存於 `%APPDATA%\FuFengMailer_Rust\`
//! （config.ini、mailContent.default、MailTemplates），檔案格式與 C++ / .NET 版相容。

pub mod core;
