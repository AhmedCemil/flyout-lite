use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Dwm::*,
        System::Registry::*,
        UI::Shell::*,
    },
};

pub unsafe fn is_exclusive_fullscreen() -> bool {
    match SHQueryUserNotificationState() {
        Ok(state) => state == QUNS_RUNNING_D3D_FULL_SCREEN,
        Err(_) => false,
    }
}

pub unsafe fn enable_mica(hwnd: HWND) {
    // Windows 11 22H2+: DWMWA_SYSTEMBACKDROP_TYPE = 38, DWMSBT_MAINWINDOW = 2 (Mica)
    let backdrop: i32 = 2;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE,
        (&backdrop as *const i32) as *const _,
        std::mem::size_of::<i32>() as u32,
    );
    // Round the corners (DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_ROUND = 2)
    let corner: i32 = 2;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        (&corner as *const i32) as *const _,
        std::mem::size_of::<i32>() as u32,
    );
    // Dark mode title bar (33+ on legacy, on Win11 the attribute is 20)
    let dark: i32 = 1;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        (&dark as *const i32) as *const _,
        std::mem::size_of::<i32>() as u32,
    );
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

/// Returns `true` if Windows is using light theme for apps, else `false` (dark).
pub fn system_uses_light_theme() -> bool {
    unsafe {
        let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let value: Vec<u16> = "AppsUseLightTheme"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), Some(0), KEY_READ, &mut hkey).is_err()
        {
            return false;
        }
        let mut data: u32 = 0;
        let mut cb: u32 = std::mem::size_of::<u32>() as u32;
        let mut data_type = REG_VALUE_TYPE::default();
        let res = RegQueryValueExW(
            hkey,
            PCWSTR(value.as_ptr()),
            None,
            Some(&mut data_type),
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut cb),
        );
        let _ = RegCloseKey(hkey);
        res.is_ok() && data == 1
    }
}
