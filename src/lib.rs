//! 福豐薪資單批量寄送工具的商業邏輯（不依賴 GUI）。
//!
//! 以 .NET 10 版（FuFengMailer_NET10）為參考規格，與其及 C++ 版共用
//! `%APPDATA%\FuFengMailer\` 下的 config.ini、mailContent.default 與 MailTemplates。

pub mod core;
