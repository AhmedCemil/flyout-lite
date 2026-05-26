#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn)]

mod app;
mod config;
mod flyout;
mod hotkey;
mod media;
mod render;
mod settings_window;
mod startup;
mod tray;
mod util;

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::Com::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::HiDpi::*,
        UI::WindowsAndMessaging::*,
    },
};

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

        let msg_hwnd = create_message_window()?;
        tray::register(msg_hwnd)?;
        media::subscribe()?;
        flyout::create()?;
        hotkey::install(msg_hwnd)?;

        run_message_loop();

        hotkey::uninstall();
        tray::unregister(msg_hwnd);
        CoUninitialize();
    }
    Ok(())
}

unsafe fn create_message_window() -> Result<HWND> {
    let class_name = w!("FlyoutLiteMsg");
    let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassW(&wc);

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        w!("FlyoutLite"),
        WINDOW_STYLE::default(),
        0, 0, 0, 0,
        Some(HWND_MESSAGE),
        None,
        Some(hinstance),
        None,
    )?;

    Ok(hwnd)
}

unsafe fn run_message_loop() {
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            m if m == tray::WM_TRAY => {
                let event = (lparam.0 & 0xFFFF) as u32;
                if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                    tray::show_menu(hwnd);
                }
                LRESULT(0)
            }
            m if m == hotkey::WM_APP_HOTKEY => {
                flyout::show();
                LRESULT(0)
            }
            WM_COMMAND => {
                match wparam.0 {
                    v if v == tray::IDM_QUIT => PostQuitMessage(0),
                    v if v == tray::IDM_RUN_AT_STARTUP => {
                        let _ = if startup::is_enabled() {
                            startup::disable()
                        } else {
                            startup::enable()
                        };
                    }
                    v if v == tray::IDM_SETTINGS => {
                        settings_window::open(hwnd);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
