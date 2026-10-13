use anyhow::{bail, Result};
use std::path::Path;

pub fn write(path: &Path, value: &str) -> Result<()> {
    let bytes = protect(value.as_bytes())?;
    super::atomic_write(path, &bytes)
}
pub fn read(path: &Path) -> Result<String> {
    Ok(String::from_utf8(unprotect(&std::fs::read(path)?)?)?)
}

#[cfg(windows)]
fn transform(input: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let source = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &source,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &source,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        bail!(
            "Windows secure storage failed: {}",
            std::io::Error::last_os_error()
        );
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(result)
}
#[cfg(windows)]
fn protect(input: &[u8]) -> Result<Vec<u8>> {
    transform(input, true)
}
#[cfg(windows)]
fn unprotect(input: &[u8]) -> Result<Vec<u8>> {
    transform(input, false)
}
#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>> {
    bail!("Persistent secrets require Windows DPAPI; Telegram storage unavailable on this development host")
}
#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>> {
    bail!("Windows DPAPI required")
}
