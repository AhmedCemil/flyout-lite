use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Direct2D::Common::D2D_RECT_F,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

use crate::app;
use crate::config::{self, Anchor};
use crate::media;
use crate::render::{HitRegions, Renderer, Theme};
use crate::util;

pub const FLYOUT_W: i32 = 360;
pub const FLYOUT_H: i32 = 116;
pub const FLYOUT_H_EXPANDED: i32 = FLYOUT_H + 36;
pub const FLYOUT_W_COMPACT: i32 = 280;
pub const FLYOUT_H_COMPACT: i32 = 64;
const ANIM_MS: u32 = 200;
const FRAME_MS: u32 = 8; // ~120fps
const TIMER_ANIM: usize = 1;
const TIMER_HOLD: usize = 2;
const TIMER_POLL: usize = 3;
const POLL_MS: u32 = 300;

static FLYOUT_HWND: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    static STATE: RefCell<Option<FlyoutState>> = const { RefCell::new(None) };
}

struct FlyoutState {
    renderer: Renderer,
    theme: Theme,
    hits: HitRegions,
    show_anim_start: Option<Instant>,
    hide_anim_start: Option<Instant>,
    visible: bool,
    seek_dragging: bool,
    last_seek_request: Option<f64>,
}

#[allow(dead_code)]
pub fn is_visible() -> bool {
    let h = FLYOUT_HWND.load(Ordering::Acquire);
    if h == 0 {
        return false;
    }
    let hwnd = HWND(h as *mut _);
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub unsafe fn create() -> Result<HWND> {
    let class_name = w!("FlyoutLiteWindow");
    let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance,
        lpszClassName: class_name,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        hbrBackground: HBRUSH(std::ptr::null_mut()),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let (x, y) = target_position(FLYOUT_W, FLYOUT_H_EXPANDED);

    let hwnd = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class_name,
        w!("FlyoutLite"),
        WS_POPUP,
        x,
        y,
        FLYOUT_W,
        FLYOUT_H_EXPANDED,
        None,
        None,
        Some(hinstance),
        None,
    )?;

    util::enable_mica(hwnd);

    let renderer = Renderer::new(hwnd)?;
    let theme = if util::system_uses_light_theme() {
        Theme::light()
    } else {
        Theme::dark()
    };
    STATE.with(|cell| {
        *cell.borrow_mut() = Some(FlyoutState {
            renderer,
            theme,
            hits: HitRegions::default(),
            show_anim_start: None,
            hide_anim_start: None,
            visible: false,
            seek_dragging: false,
            last_seek_request: None,
        });
    });

    FLYOUT_HWND.store(hwnd.0 as isize, Ordering::Release);

    // Initial paint while hidden so first show has content
    render_frame();

    Ok(hwnd)
}

pub unsafe fn show() {
    if util::is_exclusive_fullscreen() {
        return;
    }

    let h = FLYOUT_HWND.load(Ordering::Acquire);
    if h == 0 {
        return;
    }
    let hwnd = HWND(h as *mut _);

    let now = Instant::now();
    let mut already_visible = false;
    let new_theme = if util::system_uses_light_theme() {
        Theme::light()
    } else {
        Theme::dark()
    };
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            already_visible = state.visible;
            state.show_anim_start = Some(now);
            state.hide_anim_start = None;
            state.visible = true;
            state.theme = new_theme;
        }
    });

    let cfg = config::get();
    let (w, h) = flyout_size();
    let (target_x, target_y) = target_position(w, h);
    let off_y = offscreen_y(target_y);
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        target_x,
        off_y,
        w,
        h,
        SWP_NOACTIVATE,
    );

    media::poll_timeline();
    render_frame();

    if !already_visible {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }

    SetTimer(Some(hwnd), TIMER_ANIM, FRAME_MS, None);
    SetTimer(Some(hwnd), TIMER_POLL, POLL_MS, None);
    SetTimer(Some(hwnd), TIMER_HOLD, cfg.hold_ms + ANIM_MS, None);
}

pub unsafe fn hide_animated() {
    let h = FLYOUT_HWND.load(Ordering::Acquire);
    if h == 0 {
        return;
    }
    let hwnd = HWND(h as *mut _);
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            if !state.visible {
                return;
            }
            state.hide_anim_start = Some(Instant::now());
            state.show_anim_start = None;
        }
    });
    SetTimer(Some(hwnd), TIMER_ANIM, FRAME_MS, None);
    let _ = KillTimer(Some(hwnd), TIMER_HOLD);
}

unsafe fn hide_immediate() {
    let h = FLYOUT_HWND.load(Ordering::Acquire);
    if h == 0 {
        return;
    }
    let hwnd = HWND(h as *mut _);
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            state.visible = false;
            state.show_anim_start = None;
            state.hide_anim_start = None;
            state.seek_dragging = false;
        }
    });
    let _ = KillTimer(Some(hwnd), TIMER_ANIM);
    let _ = KillTimer(Some(hwnd), TIMER_POLL);
    let _ = KillTimer(Some(hwnd), TIMER_HOLD);
    let _ = ShowWindow(hwnd, SW_HIDE);
}

fn flyout_size() -> (i32, i32) {
    let cfg = config::get();
    if cfg.compact {
        (FLYOUT_W_COMPACT, FLYOUT_H_COMPACT)
    } else {
        (FLYOUT_W, FLYOUT_H_EXPANDED)
    }
}

fn target_position(width: i32, height: i32) -> (i32, i32) {
    unsafe {
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        let cfg = config::get();
        let mx = cfg.margin_x;
        let my = cfg.margin_y;
        match cfg.anchor {
            Anchor::TopLeft => (mx, my),
            Anchor::TopCenter => ((sw - width) / 2, my),
            Anchor::TopRight => (sw - width - mx, my),
            Anchor::BottomLeft => (mx, sh - height - my),
            Anchor::BottomCenter => ((sw - width) / 2, sh - height - my),
            Anchor::BottomRight => (sw - width - mx, sh - height - my),
            Anchor::Custom => (cfg.custom_x, cfg.custom_y),
        }
    }
}

fn offscreen_y(target_y: i32) -> i32 {
    // Slide from above if anchored to top half, from below otherwise.
    let cfg = config::get();
    let from_top = matches!(
        cfg.anchor,
        Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight
    ) || (matches!(cfg.anchor, Anchor::Custom) && target_y < 200);
    if from_top {
        target_y - 40
    } else {
        target_y + 40
    }
}

fn render_frame() {
    let track = app::snapshot();
    let cfg = config::get();
    // Always show the seek bar so the flyout has a consistent size and the user
    // sees position/duration when available. Some players (e.g. Deezer) don't
    // expose timeline data — in that case the bar is empty and shows "-:--".
    let seekbar_enabled = !cfg.compact;
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            unsafe {
                if let Ok(hits) = state.renderer.render(
                    &state.theme,
                    &track.title,
                    &track.artist,
                    track.playing,
                    track.position_secs,
                    track.duration_secs,
                    seekbar_enabled,
                    cfg.compact,
                ) {
                    state.hits = hits;
                }
                if track.has_thumbnail {
                    state
                        .renderer
                        .set_album_bitmap_from_bytes(&track.thumbnail_key, &track.thumbnail_bytes);
                } else {
                    state.renderer.clear_album();
                }
            }
        }
    });
}

fn animation_tick(hwnd: HWND) -> bool {
    let mut still_animating = false;
    let mut completed_hide = false;

    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            let now = Instant::now();
            let (fw, fh) = flyout_size();
            let (target_x, target_y) = target_position(fw, fh);
            let off_y = offscreen_y(target_y);

            if let Some(start) = state.show_anim_start {
                let elapsed = now.duration_since(start).as_millis() as f32;
                let t = (elapsed / ANIM_MS as f32).clamp(0.0, 1.0);
                let eased = util::ease_out_cubic(t);
                let y = off_y as f32 + (target_y - off_y) as f32 * eased;
                let alpha = (255.0 * eased) as u8;
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        target_x,
                        y as i32,
                        fw,
                        fh,
                        SWP_NOACTIVATE | SWP_NOREDRAW,
                    );
                    set_alpha(hwnd, alpha);
                }
                if t >= 1.0 {
                    state.show_anim_start = None;
                } else {
                    still_animating = true;
                }
            }

            if let Some(start) = state.hide_anim_start {
                let elapsed = now.duration_since(start).as_millis() as f32;
                let t = (elapsed / ANIM_MS as f32).clamp(0.0, 1.0);
                let eased = util::ease_out_cubic(t);
                let y = target_y as f32 + (off_y - target_y) as f32 * eased;
                let alpha = (255.0 * (1.0 - eased)) as u8;
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        target_x,
                        y as i32,
                        fw,
                        fh,
                        SWP_NOACTIVATE | SWP_NOREDRAW,
                    );
                    set_alpha(hwnd, alpha);
                }
                if t >= 1.0 {
                    state.hide_anim_start = None;
                    state.visible = false;
                    completed_hide = true;
                } else {
                    still_animating = true;
                }
            }
        }
    });

    if completed_hide {
        unsafe {
            hide_immediate();
        }
    }
    still_animating
}

unsafe fn set_alpha(hwnd: HWND, alpha: u8) {
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    if (ex as u32) & WS_EX_LAYERED.0 == 0 {
        // Mica + layered doesn't compose well — toggle layered only during animation
    }
    // We use DirectComposition opacity instead via visual? For simplicity scale by re-rendering.
    // Skip alpha on Mica path; show/hide is timing-based.
    let _ = (hwnd, alpha);
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => match wparam.0 {
                v if v == TIMER_ANIM => {
                    if !animation_tick(hwnd) {
                        let _ = KillTimer(Some(hwnd), TIMER_ANIM);
                    }
                    LRESULT(0)
                }
                v if v == TIMER_POLL => {
                    media::poll_timeline();
                    render_frame();
                    LRESULT(0)
                }
                v if v == TIMER_HOLD => {
                    let _ = KillTimer(Some(hwnd), TIMER_HOLD);
                    hide_animated();
                    LRESULT(0)
                }
                _ => LRESULT(0),
            },
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as i16 as f32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                handle_click(hwnd, x, y);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let dragging = STATE
                    .with(|cell| cell.borrow().as_ref().map(|s| s.seek_dragging).unwrap_or(false));
                if dragging {
                    let x = (lparam.0 & 0xFFFF) as i16 as f32;
                    let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                    handle_seek_drag(x, y);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let mut should_seek = None;
                STATE.with(|cell| {
                    if let Some(state) = cell.borrow_mut().as_mut() {
                        if state.seek_dragging {
                            should_seek = state.last_seek_request.take();
                            state.seek_dragging = false;
                        }
                    }
                });
                if let Some(secs) = should_seek {
                    media::try_seek(secs);
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                render_frame();
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn handle_click(hwnd: HWND, x: f32, y: f32) {
    let mut hit_seek = false;
    let mut seek_target = None;

    let action = STATE.with(|cell| -> Option<&'static str> {
        let state = cell.borrow();
        let s = state.as_ref()?;
        if point_in_rect(x, y, &s.hits.prev) {
            return Some("prev");
        }
        if point_in_rect(x, y, &s.hits.play_pause) {
            return Some("play_pause");
        }
        if point_in_rect(x, y, &s.hits.next) {
            return Some("next");
        }
        if point_in_rect(x, y, &s.hits.seek_track) {
            return Some("seek");
        }
        None
    });

    match action {
        Some("prev") => media::try_prev(),
        Some("play_pause") => media::try_play_pause(),
        Some("next") => media::try_next(),
        Some("seek") => {
            hit_seek = true;
            STATE.with(|cell| {
                if let Some(state) = cell.borrow_mut().as_mut() {
                    state.seek_dragging = true;
                    seek_target = compute_seek_seconds(x, &state.hits.seek_track);
                    state.last_seek_request = seek_target;
                }
            });
        }
        _ => {}
    }

    if hit_seek {
        if let Some(secs) = seek_target {
            media::try_seek(secs);
        }
    }

    // Refresh hold timer on user interaction
    let cfg = config::get();
    SetTimer(Some(hwnd), TIMER_HOLD, cfg.hold_ms + ANIM_MS, None);
    render_frame();
}

fn handle_seek_drag(x: f32, _y: f32) {
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            if let Some(secs) = compute_seek_seconds(x, &state.hits.seek_track) {
                state.last_seek_request = Some(secs);
                // Reflect in app state for smooth UI feedback
                let track = app::snapshot();
                app::update_timeline(secs, track.duration_secs, track.seekable);
            }
        }
    });
    render_frame();
}

fn compute_seek_seconds(x: f32, track: &D2D_RECT_F) -> Option<f64> {
    let width = track.right - track.left;
    if width <= 0.0 {
        return None;
    }
    let rel = ((x - track.left) / width).clamp(0.0, 1.0);
    let info = app::snapshot();
    if info.duration_secs <= 0.0 {
        return None;
    }
    Some(rel as f64 * info.duration_secs)
}

fn point_in_rect(x: f32, y: f32, r: &D2D_RECT_F) -> bool {
    x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}
