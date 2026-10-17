//! 密碼以 Windows DPAPI（CurrentUser）加密：只有同一台電腦的同一個 Windows 帳號能解密。
//! 格式 `dpapi:<base64>`、entropy 與 .NET 版（ProtectedData）相同，兩版可互相讀取已存的密碼。

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

const PREFIX: &str = "dpapi:";
const ENTROPY: &[u8] = b"FuFengMailer.SmtpPassword";

pub fn protect(password: &str) -> Option<String> {
    let encrypted = imp::protect(password.as_bytes())?;
    Some(format!("{PREFIX}{}", STANDARD.encode(encrypted)))
}

/// 解密已存的密碼；格式不符或無法解密（例如換了 Windows 帳號）時回傳 None。
pub fn unprotect(stored: &str) -> Option<String> {
    let data = STANDARD.decode(stored.strip_prefix(PREFIX)?).ok()?;
    String::from_utf8(imp::unprotect(&data)?).ok()
}

#[cfg(windows)]
mod imp {
    use super::ENTROPY;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    /// SAFETY: out 由 CryptProtectData/CryptUnprotectData 以 LocalAlloc 配置，複製後以 LocalFree 釋放
    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        unsafe {
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData.cast());
            v
        }
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        let input = blob(data);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: 所有指標在呼叫期間皆有效
        let ok = unsafe {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        (ok != 0).then(|| unsafe { take(out) })
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        let input = blob(data);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: 所有指標在呼叫期間皆有效
        let ok = unsafe {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        (ok != 0).then(|| unsafe { take(out) })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
    pub fn unprotect(_: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn round_trip_without_plaintext() {
        let stored = protect("p@ss 密碼").unwrap();
        assert!(stored.starts_with("dpapi:"));
        assert!(!stored.contains("p@ss"));
        assert_eq!(unprotect(&stored).as_deref(), Some("p@ss 密碼"));
    }

    #[test]
    fn invalid_values_are_ignored() {
        assert_eq!(unprotect("plaintext"), None);
        assert_eq!(unprotect("dpapi:not-base64!"), None);
        assert_eq!(unprotect("dpapi:AAAA"), None);
    }
}
