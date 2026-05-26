use windows::{
    core::*,
    Win32::System::Registry::*,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "FlyoutLite";

unsafe fn open_run_key(write: bool) -> Result<HKEY> {
    let subkey_w: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = HKEY::default();
    let access = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
    RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey_w.as_ptr()), Some(0), access, &mut hkey).ok()?;
    Ok(hkey)
}

pub fn is_enabled() -> bool {
    unsafe {
        let Ok(hkey) = open_run_key(false) else { return false };
        let value_w: Vec<u16> = VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let mut data_type = REG_VALUE_TYPE::default();
        let mut cb: u32 = 0;
        let res = RegQueryValueExW(
            hkey,
            PCWSTR(value_w.as_ptr()),
            None,
            Some(&mut data_type),
            None,
            Some(&mut cb),
        );
        let _ = RegCloseKey(hkey);
        res.is_ok() && cb > 0
    }
}

pub fn enable() -> Result<()> {
    unsafe {
        let path = std::env::current_exe()?;
        let quoted = format!("\"{}\"", path.display());
        let value_w: Vec<u16> = VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let data_w: Vec<u16> = quoted.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes: &[u8] = std::slice::from_raw_parts(
            data_w.as_ptr() as *const u8,
            data_w.len() * 2,
        );
        let hkey = open_run_key(true)?;
        let res = RegSetValueExW(
            hkey,
            PCWSTR(value_w.as_ptr()),
            None,
            REG_SZ,
            Some(bytes),
        );
        let _ = RegCloseKey(hkey);
        res.ok()?;
        Ok(())
    }
}

pub fn disable() -> Result<()> {
    unsafe {
        let value_w: Vec<u16> = VALUE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let hkey = open_run_key(true)?;
        let res = RegDeleteValueW(hkey, PCWSTR(value_w.as_ptr()));
        let _ = RegCloseKey(hkey);
        res.ok()?;
        Ok(())
    }
}
