use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::Shell::*,
        UI::WindowsAndMessaging::*,
    },
};

use crate::startup;

pub const WM_TRAY: u32 = WM_USER + 1;
pub const IDM_QUIT: usize = 1001;
pub const IDM_RUN_AT_STARTUP: usize = 1002;
pub const IDM_SETTINGS: usize = 1003;
const TRAY_ID: u32 = 1;
const IDI_APP_ICON: u16 = 1;

pub unsafe fn register(hwnd: HWND) -> Result<()> {
    let nid = data(hwnd, true);
    Shell_NotifyIconW(NIM_ADD, &nid).ok()?;
    let mut ver = nid;
    ver.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    let _ = Shell_NotifyIconW(NIM_SETVERSION, &ver);
    Ok(())
}

pub unsafe fn unregister(hwnd: HWND) {
    let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd, false));
}

pub unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };
    let startup_flag = if startup::is_enabled() {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };
    let _ = AppendMenuW(menu, MF_STRING, IDM_SETTINGS, w!("Settings..."));
    let _ = AppendMenuW(menu, startup_flag, IDM_RUN_AT_STARTUP, w!("Run at startup"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, IDM_QUIT, w!("Quit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
        pt.x,
        pt.y,
        None,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
}

fn data(hwnd: HWND, with_icon: bool) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    };

    if with_icon {
        unsafe {
            if let Ok(hinstance) = GetModuleHandleW(None) {
                if let Ok(ico) =
                    LoadIconW(Some(hinstance.into()), PCWSTR(IDI_APP_ICON as *const u16))
                {
                    nid.hIcon = ico;
                }
            }
        }
    }

    let tip = "FlyoutLite";
    let tip_bytes: Vec<u16> = tip.encode_utf16().collect();
    let len = tip_bytes.len().min(nid.szTip.len() - 1);
    nid.szTip[..len].copy_from_slice(&tip_bytes[..len]);

    nid
}
