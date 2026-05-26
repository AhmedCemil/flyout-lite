use std::sync::atomic::{AtomicIsize, Ordering};

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::Input::KeyboardAndMouse::*,
        UI::WindowsAndMessaging::*,
    },
};

pub const WM_APP_HOTKEY: u32 = WM_APP + 1;

pub const HK_PLAY_PAUSE: u32 = 1;
pub const HK_NEXT: u32 = 2;
pub const HK_PREV: u32 = 3;
pub const HK_VOL_UP: u32 = 4;
pub const HK_VOL_DOWN: u32 = 5;
pub const HK_VOL_MUTE: u32 = 6;

static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);
static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);

pub unsafe fn install(target: HWND) -> Result<()> {
    TARGET_HWND.store(target.0 as isize, Ordering::Release);

    let hinstance = GetModuleHandleW(None)?;
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_proc), Some(hinstance.into()), 0)?;
    HOOK_HANDLE.store(hook.0 as isize, Ordering::Release);
    Ok(())
}

pub unsafe fn uninstall() {
    let h = HOOK_HANDLE.swap(0, Ordering::AcqRel);
    if h != 0 {
        let _ = UnhookWindowsHookEx(HHOOK(h as *mut _));
    }
}

extern "system" fn low_level_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32
            && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN)
        {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = VIRTUAL_KEY(info.vkCode as u16);
            let id = match vk {
                VK_MEDIA_PLAY_PAUSE => Some(HK_PLAY_PAUSE),
                VK_MEDIA_NEXT_TRACK => Some(HK_NEXT),
                VK_MEDIA_PREV_TRACK => Some(HK_PREV),
                VK_VOLUME_UP => Some(HK_VOL_UP),
                VK_VOLUME_DOWN => Some(HK_VOL_DOWN),
                VK_VOLUME_MUTE => Some(HK_VOL_MUTE),
                _ => None,
            };

            if let Some(id) = id {
                let target = TARGET_HWND.load(Ordering::Acquire);
                if target != 0 {
                    let _ = PostMessageW(
                        Some(HWND(target as *mut _)),
                        WM_APP_HOTKEY,
                        WPARAM(id as usize),
                        LPARAM(0),
                    );
                }
            }
        }

        let next_hook = HOOK_HANDLE.load(Ordering::Acquire);
        CallNextHookEx(Some(HHOOK(next_hook as *mut _)), code, wparam, lparam)
    }
}
