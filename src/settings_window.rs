use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::*,
            Direct3D11::*,
            DirectComposition::*,
            DirectWrite::*,
            Dxgi::{Common::*, *},
        },
        System::{Com::*, LibraryLoader::GetModuleHandleW},
        UI::Input::KeyboardAndMouse::*,
        UI::WindowsAndMessaging::*,
    },
};

use windows_numerics::Vector2;

use crate::config::{self, Anchor, Config};
use crate::flyout;
use crate::startup;
use crate::util;

const CLIENT_W: i32 = 480;
const CLIENT_H: i32 = 540;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static CLIENT_SIZE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn pack_size(w: i32, h: i32) -> u64 {
    ((w as u32 as u64) << 32) | (h as u32 as u64)
}
fn unpack_size(v: u64) -> (i32, i32) {
    (((v >> 32) & 0xFFFF_FFFF) as i32, (v & 0xFFFF_FFFF) as i32)
}
fn client_size() -> (i32, i32) {
    let v = CLIENT_SIZE.load(Ordering::Acquire);
    if v == 0 { (CLIENT_W, CLIENT_H) } else { unpack_size(v) }
}

#[derive(Clone, Copy, Default)]
struct Hits {
    anchors: [D2D_RECT_F; 7],
    field_mx: D2D_RECT_F,
    field_my: D2D_RECT_F,
    field_cx: D2D_RECT_F,
    field_cy: D2D_RECT_F,
    field_hold: D2D_RECT_F,
    toggle_compact: D2D_RECT_F,
    toggle_startup: D2D_RECT_F,
    btn_preview: D2D_RECT_F,
    btn_save: D2D_RECT_F,
    btn_close: D2D_RECT_F,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    None,
    Mx,
    My,
    Cx,
    Cy,
    Hold,
}

const FOOTER_H: f32 = 70.0;
const TITLE_H: f32 = 56.0;

struct State {
    // D2D pipeline
    d2d_factory: ID2D1Factory1,
    dwrite_factory: IDWriteFactory,
    #[allow(dead_code)]
    d2d_device: ID2D1Device,
    d2d_context: ID2D1DeviceContext,
    swapchain: IDXGISwapChain1,
    dcomp_device: IDCompositionDevice,
    #[allow(dead_code)]
    dcomp_target: IDCompositionTarget,
    #[allow(dead_code)]
    dcomp_visual: IDCompositionVisual,
    #[allow(dead_code)]
    target_bitmap: ID2D1Bitmap1,

    title_format: IDWriteTextFormat,
    label_format: IDWriteTextFormat,
    body_format: IDWriteTextFormat,
    small_format: IDWriteTextFormat,
    icon_format: IDWriteTextFormat,

    cfg: Config,
    hits: Hits,
    focus: Field,
    saved_flash: u32, // frames left to flash the Save button
    startup_enabled: bool,
    scroll_y: f32,
    content_h: f32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn open(_owner: HWND) {
    let existing = WINDOW.load(Ordering::Acquire);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    unsafe {
        let _ = create();
    }
}

unsafe fn create() -> Result<()> {
    let class_name = w!("FlyoutLiteSettings");
    let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance,
        lpszClassName: class_name,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(std::ptr::null_mut()),
        style: CS_HREDRAW | CS_VREDRAW,
        ..Default::default()
    };
    RegisterClassW(&wc);

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let mut rc = RECT { left: 0, top: 0, right: CLIENT_W, bottom: CLIENT_H };
    let _ = AdjustWindowRectEx(&mut rc, style, false, WINDOW_EX_STYLE::default());
    let win_w = rc.right - rc.left;
    let win_h = rc.bottom - rc.top;

    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    let x = (sw - win_w) / 2;
    let y = (sh - win_h) / 2;

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        w!("FlyoutLite — Settings"),
        style,
        x,
        y,
        win_w,
        win_h,
        None,
        None,
        Some(hinstance),
        None,
    )?;

    // Store actual client size for the renderer.
    let mut cr = RECT::default();
    let _ = GetClientRect(hwnd, &mut cr);
    let cw = (cr.right - cr.left).max(1);
    let ch = (cr.bottom - cr.top).max(1);
    CLIENT_SIZE.store(pack_size(cw, ch), Ordering::Release);

    util::enable_mica(hwnd);
    WINDOW.store(hwnd.0 as isize, Ordering::Release);

    let renderer = build_renderer(hwnd)?;
    STATE.with(|cell| {
        *cell.borrow_mut() = Some(State {
            d2d_factory: renderer.0,
            dwrite_factory: renderer.1,
            d2d_device: renderer.2,
            d2d_context: renderer.3,
            swapchain: renderer.4,
            dcomp_device: renderer.5,
            dcomp_target: renderer.6,
            dcomp_visual: renderer.7,
            target_bitmap: renderer.8,
            title_format: renderer.9,
            label_format: renderer.10,
            body_format: renderer.11,
            small_format: renderer.12,
            icon_format: renderer.13,
            cfg: config::get(),
            hits: Hits::default(),
            focus: Field::None,
            saved_flash: 0,
            startup_enabled: startup::is_enabled(),
            scroll_y: 0.0,
            content_h: 0.0,
        });
    });

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    paint(hwnd);
    Ok(())
}

#[allow(clippy::type_complexity)]
unsafe fn build_renderer(
    hwnd: HWND,
) -> Result<(
    ID2D1Factory1,
    IDWriteFactory,
    ID2D1Device,
    ID2D1DeviceContext,
    IDXGISwapChain1,
    IDCompositionDevice,
    IDCompositionTarget,
    IDCompositionVisual,
    ID2D1Bitmap1,
    IDWriteTextFormat,
    IDWriteTextFormat,
    IDWriteTextFormat,
    IDWriteTextFormat,
    IDWriteTextFormat,
)> {
    let d2d_factory: ID2D1Factory1 = D2D1CreateFactory::<ID2D1Factory1>(
        D2D1_FACTORY_TYPE_SINGLE_THREADED,
        Some(&D2D1_FACTORY_OPTIONS::default()),
    )?;
    let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

    let mut d3d_device: Option<ID3D11Device> = None;
    D3D11CreateDevice(
        None,
        D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        None,
        D3D11_SDK_VERSION,
        Some(&mut d3d_device),
        None,
        None,
    )?;
    let d3d_device = d3d_device.unwrap();
    let dxgi_device: IDXGIDevice = d3d_device.cast()?;

    let d2d_device: ID2D1Device = d2d_factory.CreateDevice(&dxgi_device)?;
    let d2d_context: ID2D1DeviceContext =
        d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

    let (cw, ch) = client_size();
    let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS::default())?;
    let swap_desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: cw as u32,
        Height: ch as u32,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: BOOL(0),
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        Flags: 0,
    };
    let swapchain: IDXGISwapChain1 =
        dxgi_factory.CreateSwapChainForComposition(&dxgi_device, &swap_desc, None)?;

    let dcomp_device: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
    let dcomp_target = dcomp_device.CreateTargetForHwnd(hwnd, true)?;
    let dcomp_visual = dcomp_device.CreateVisual()?;
    dcomp_visual.SetContent(&swapchain)?;
    dcomp_target.SetRoot(&dcomp_visual)?;
    dcomp_device.Commit()?;

    let back: IDXGISurface = swapchain.GetBuffer(0)?;
    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        colorContext: std::mem::ManuallyDrop::new(None),
    };
    let target_bitmap = d2d_context.CreateBitmapFromDxgiSurface(&back, Some(&props))?;
    d2d_context.SetTarget(&target_bitmap);

    let title_format = make_text(&dwrite_factory, "Segoe UI Variable Display", 22.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
    let label_format = make_text(&dwrite_factory, "Segoe UI Variable Text", 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
    let body_format = make_text(&dwrite_factory, "Segoe UI Variable Text", 13.0, DWRITE_FONT_WEIGHT_NORMAL)?;
    let small_format = make_text(&dwrite_factory, "Segoe UI Variable Small", 11.0, DWRITE_FONT_WEIGHT_NORMAL)?;
    let icon_format = make_text(&dwrite_factory, "Segoe UI Variable Text", 18.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;

    Ok((
        d2d_factory,
        dwrite_factory,
        d2d_device,
        d2d_context,
        swapchain,
        dcomp_device,
        dcomp_target,
        dcomp_visual,
        target_bitmap,
        title_format,
        label_format,
        body_format,
        small_format,
        icon_format,
    ))
}

unsafe fn make_text(
    factory: &IDWriteFactory,
    family: &str,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
) -> Result<IDWriteTextFormat> {
    let fam: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    let loc: Vec<u16> = "en-us".encode_utf16().chain(std::iter::once(0)).collect();
    factory.CreateTextFormat(
        PCWSTR(fam.as_ptr()),
        None,
        weight,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        size,
        PCWSTR(loc.as_ptr()),
    )
}

#[derive(Clone, Copy)]
struct Pal {
    bg: D2D1_COLOR_F,
    panel: D2D1_COLOR_F,
    panel_hover: D2D1_COLOR_F,
    text: D2D1_COLOR_F,
    text_dim: D2D1_COLOR_F,
    accent: D2D1_COLOR_F,
    accent_text: D2D1_COLOR_F,
    stroke: D2D1_COLOR_F,
    field_bg: D2D1_COLOR_F,
    field_border: D2D1_COLOR_F,
    field_focus: D2D1_COLOR_F,
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}

fn palette() -> Pal {
    if util::system_uses_light_theme() {
        Pal {
            bg: rgba(0xF3, 0xF3, 0xF3, 0xF2),
            panel: rgba(0xFF, 0xFF, 0xFF, 0xE0),
            panel_hover: rgba(0x00, 0x00, 0x00, 0x12),
            text: rgba(0x10, 0x10, 0x10, 0xFF),
            text_dim: rgba(0x10, 0x10, 0x10, 0x99),
            accent: rgba(0x00, 0x67, 0xC0, 0xFF),
            accent_text: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            stroke: rgba(0x00, 0x00, 0x00, 0x1F),
            field_bg: rgba(0xFF, 0xFF, 0xFF, 0xC0),
            field_border: rgba(0x00, 0x00, 0x00, 0x33),
            field_focus: rgba(0x00, 0x67, 0xC0, 0xFF),
        }
    } else {
        Pal {
            bg: rgba(0x1F, 0x1F, 0x1F, 0xF2),
            panel: rgba(0xFF, 0xFF, 0xFF, 0x10),
            panel_hover: rgba(0xFF, 0xFF, 0xFF, 0x1A),
            text: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            text_dim: rgba(0xFF, 0xFF, 0xFF, 0x99),
            accent: rgba(0x4C, 0xC2, 0xFF, 0xFF),
            accent_text: rgba(0x00, 0x00, 0x00, 0xE6),
            stroke: rgba(0xFF, 0xFF, 0xFF, 0x1F),
            field_bg: rgba(0xFF, 0xFF, 0xFF, 0x10),
            field_border: rgba(0xFF, 0xFF, 0xFF, 0x33),
            field_focus: rgba(0x4C, 0xC2, 0xFF, 0xFF),
        }
    }
}

fn anchor_to_index(a: Anchor) -> usize {
    match a {
        Anchor::TopLeft => 0,
        Anchor::TopCenter => 1,
        Anchor::TopRight => 2,
        Anchor::BottomLeft => 3,
        Anchor::BottomCenter => 4,
        Anchor::BottomRight => 5,
        Anchor::Custom => 6,
    }
}

fn index_to_anchor(i: usize) -> Anchor {
    match i {
        0 => Anchor::TopLeft,
        1 => Anchor::TopCenter,
        2 => Anchor::TopRight,
        3 => Anchor::BottomLeft,
        4 => Anchor::BottomCenter,
        5 => Anchor::BottomRight,
        _ => Anchor::Custom,
    }
}

fn anchor_glyph(i: usize) -> &'static str {
    // Geometric arrows from the standard Unicode block — render reliably in
    // Segoe UI Variable Text and unambiguously indicate direction.
    match i {
        0 => "\u{2196}", // ↖ up-left
        1 => "\u{2191}", // ↑ up
        2 => "\u{2197}", // ↗ up-right
        3 => "\u{2199}", // ↙ down-left
        4 => "\u{2193}", // ↓ down
        5 => "\u{2198}", // ↘ down-right
        _ => "\u{22EF}", // ⋯ custom
    }
}

fn anchor_label(i: usize) -> &'static str {
    match i {
        0 => "Top Left",
        1 => "Top Center",
        2 => "Top Right",
        3 => "Bottom Left",
        4 => "Bottom Center",
        5 => "Bottom Right",
        _ => "Custom (X / Y)",
    }
}

fn paint(hwnd: HWND) {
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            unsafe {
                let _ = render(hwnd, state);
            }
        }
    });
}

unsafe fn render(_hwnd: HWND, s: &mut State) -> Result<()> {
    let pal = palette();
    let ctx = &s.d2d_context;
    ctx.BeginDraw();
    ctx.Clear(Some(&rgba(0, 0, 0, 0)));

    let (cw_i, ch_i) = client_size();
    let w = cw_i as f32;
    let h = ch_i as f32;

    // background panel with rounded corners
    let bg_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.bg, None)?;
    let rr = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: h },
        radiusX: 0.0,
        radiusY: 0.0,
    };
    ctx.FillRoundedRectangle(&rr, &bg_brush);

    let text_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.text, None)?;
    let dim_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.text_dim, None)?;
    let accent_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.accent, None)?;
    let panel_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.panel, None)?;
    let stroke_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.stroke, None)?;

    let pad = 20.0;

    // Sticky title above the scroll viewport
    draw_text(ctx, &s.dwrite_factory, "Settings", &s.title_format, &D2D_RECT_F {
        left: pad, top: 14.0, right: w - pad, bottom: 14.0 + 32.0,
    }, &text_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;

    // Scroll viewport spans (TITLE_H .. h - FOOTER_H)
    let viewport_top = TITLE_H;
    let viewport_bottom = h - FOOTER_H;
    let viewport_h = viewport_bottom - viewport_top;

    // Apply translation for scrolling; clip to viewport.
    ctx.PushAxisAlignedClip(
        &D2D_RECT_F { left: 0.0, top: viewport_top, right: w, bottom: viewport_bottom },
        D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    );
    let scroll = s.scroll_y;
    ctx.SetTransform(&windows_numerics::Matrix3x2 {
        M11: 1.0, M12: 0.0,
        M21: 0.0, M22: 1.0,
        M31: 0.0, M32: viewport_top - scroll,
    });

    // From here on `y` is relative to the start of the scrollable content.
    let mut y: f32 = 0.0;

    // Position section header
    draw_text(ctx, &s.dwrite_factory, "Position", &s.label_format, &D2D_RECT_F {
        left: pad, top: y, right: w - pad, bottom: y + 18.0,
    }, &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    y += 22.0;

    // 3x3 anchor grid (preset cells, 6 presets in 2 rows of 3; 7th = Custom card below)
    let grid_left = pad;
    let grid_top = y;
    let cell_w = (w - pad * 2.0 - 16.0) / 3.0;
    let cell_h = 56.0;
    let gap = 8.0;
    let selected_idx = anchor_to_index(s.cfg.anchor);

    for i in 0..6 {
        let col = (i % 3) as f32;
        let row = (i / 3) as f32;
        let r = D2D_RECT_F {
            left: grid_left + col * (cell_w + gap),
            top: grid_top + row * (cell_h + gap),
            right: grid_left + col * (cell_w + gap) + cell_w,
            bottom: grid_top + row * (cell_h + gap) + cell_h,
        };
        let selected = i == selected_idx;
        draw_anchor_cell(ctx, &s.dwrite_factory, &s.icon_format, &s.small_format, &r, anchor_glyph(i), anchor_label(i), selected, &pal)?;
        s.hits.anchors[i] = r;
    }

    y = grid_top + 2.0 * (cell_h + gap) + 4.0;

    // Custom anchor row with X/Y fields
    let custom_r = D2D_RECT_F {
        left: grid_left,
        top: y,
        right: w - pad,
        bottom: y + cell_h,
    };
    let custom_selected = selected_idx == 6;
    draw_panel(ctx, &custom_r, custom_selected, &pal)?;
    s.hits.anchors[6] = D2D_RECT_F {
        left: custom_r.left,
        top: custom_r.top,
        right: custom_r.left + 200.0,
        bottom: custom_r.bottom,
    };

    // "Custom" label + radio dot
    let radio_cx = custom_r.left + 18.0;
    let radio_cy = (custom_r.top + custom_r.bottom) / 2.0;
    draw_radio(ctx, radio_cx, radio_cy, custom_selected, &pal)?;
    draw_text(ctx, &s.dwrite_factory, "Custom",
        &s.body_format,
        &D2D_RECT_F { left: radio_cx + 14.0, top: custom_r.top + 18.0, right: radio_cx + 90.0, bottom: custom_r.bottom },
        &text_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;

    // X / Y fields
    let field_w = 70.0;
    let field_h = 28.0;
    let fy = (custom_r.top + custom_r.bottom) / 2.0 - field_h / 2.0;
    let mut fx = custom_r.right - 12.0 - field_w * 2.0 - 30.0 - 12.0;

    draw_text(ctx, &s.dwrite_factory, "X", &s.small_format,
        &D2D_RECT_F { left: fx - 14.0, top: fy + 5.0, right: fx, bottom: fy + field_h },
        &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    let r_cx = D2D_RECT_F { left: fx, top: fy, right: fx + field_w, bottom: fy + field_h };
    draw_field(ctx, &s.dwrite_factory, &s.body_format, &r_cx, &s.cfg.custom_x.to_string(), s.focus == Field::Cx, &pal, &text_brush)?;
    s.hits.field_cx = r_cx;
    fx += field_w + 24.0;
    draw_text(ctx, &s.dwrite_factory, "Y", &s.small_format,
        &D2D_RECT_F { left: fx - 14.0, top: fy + 5.0, right: fx, bottom: fy + field_h },
        &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    let r_cy = D2D_RECT_F { left: fx, top: fy, right: fx + field_w, bottom: fy + field_h };
    draw_field(ctx, &s.dwrite_factory, &s.body_format, &r_cy, &s.cfg.custom_y.to_string(), s.focus == Field::Cy, &pal, &text_brush)?;
    s.hits.field_cy = r_cy;

    y += cell_h + 16.0;

    // Margin row
    draw_text(ctx, &s.dwrite_factory, "Margin", &s.label_format,
        &D2D_RECT_F { left: pad, top: y, right: w - pad, bottom: y + 18.0 },
        &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    y += 22.0;

    let row_r = D2D_RECT_F { left: pad, top: y, right: w - pad, bottom: y + 44.0 };
    draw_panel_static(ctx, &row_r, &panel_brush, &stroke_brush)?;
    let fy2 = row_r.top + (row_r.bottom - row_r.top) / 2.0 - field_h / 2.0;
    let mut fx2 = row_r.left + 16.0;

    draw_text(ctx, &s.dwrite_factory, "X", &s.small_format,
        &D2D_RECT_F { left: fx2, top: fy2 + 5.0, right: fx2 + 14.0, bottom: fy2 + field_h },
        &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    fx2 += 16.0;
    let r_mx = D2D_RECT_F { left: fx2, top: fy2, right: fx2 + field_w, bottom: fy2 + field_h };
    draw_field(ctx, &s.dwrite_factory, &s.body_format, &r_mx, &s.cfg.margin_x.to_string(), s.focus == Field::Mx, &pal, &text_brush)?;
    s.hits.field_mx = r_mx;
    fx2 += field_w + 24.0;
    draw_text(ctx, &s.dwrite_factory, "Y", &s.small_format,
        &D2D_RECT_F { left: fx2, top: fy2 + 5.0, right: fx2 + 14.0, bottom: fy2 + field_h },
        &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    fx2 += 16.0;
    let r_my = D2D_RECT_F { left: fx2, top: fy2, right: fx2 + field_w, bottom: fy2 + field_h };
    draw_field(ctx, &s.dwrite_factory, &s.body_format, &r_my, &s.cfg.margin_y.to_string(), s.focus == Field::My, &pal, &text_brush)?;
    s.hits.field_my = r_my;

    y += 50.0;

    // Behavior section
    draw_text(ctx, &s.dwrite_factory, "Behavior", &s.label_format,
        &D2D_RECT_F { left: pad, top: y, right: w - pad, bottom: y + 18.0 },
        &dim_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    y += 22.0;

    // Hold ms row
    let hold_r = D2D_RECT_F { left: pad, top: y, right: w - pad, bottom: y + 44.0 };
    draw_panel_static(ctx, &hold_r, &panel_brush, &stroke_brush)?;
    draw_text(ctx, &s.dwrite_factory, "Visible duration", &s.body_format,
        &D2D_RECT_F { left: hold_r.left + 16.0, top: hold_r.top + 13.0, right: hold_r.right - 130.0, bottom: hold_r.bottom },
        &text_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    let r_hold = D2D_RECT_F {
        left: hold_r.right - 16.0 - 100.0,
        top: hold_r.top + (hold_r.bottom - hold_r.top) / 2.0 - field_h / 2.0,
        right: hold_r.right - 16.0,
        bottom: hold_r.top + (hold_r.bottom - hold_r.top) / 2.0 + field_h / 2.0,
    };
    draw_field(ctx, &s.dwrite_factory, &s.body_format, &r_hold, &format!("{} ms", s.cfg.hold_ms), s.focus == Field::Hold, &pal, &text_brush)?;
    s.hits.field_hold = r_hold;
    y += 50.0;

    // Compact toggle
    let compact_r = D2D_RECT_F { left: pad, top: y, right: w - pad, bottom: y + 44.0 };
    draw_panel_static(ctx, &compact_r, &panel_brush, &stroke_brush)?;
    draw_text(ctx, &s.dwrite_factory, "Compact layout", &s.body_format,
        &D2D_RECT_F { left: compact_r.left + 16.0, top: compact_r.top + 13.0, right: compact_r.right - 80.0, bottom: compact_r.bottom },
        &text_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    draw_toggle(ctx, &D2D_RECT_F {
        left: compact_r.right - 60.0,
        top: compact_r.top + 12.0,
        right: compact_r.right - 16.0,
        bottom: compact_r.bottom - 12.0,
    }, s.cfg.compact, &pal)?;
    s.hits.toggle_compact = compact_r;
    y += 50.0;

    // Startup toggle
    let startup_r = D2D_RECT_F { left: pad, top: y, right: w - pad, bottom: y + 44.0 };
    draw_panel_static(ctx, &startup_r, &panel_brush, &stroke_brush)?;
    draw_text(ctx, &s.dwrite_factory, "Run at startup", &s.body_format,
        &D2D_RECT_F { left: startup_r.left + 16.0, top: startup_r.top + 13.0, right: startup_r.right - 80.0, bottom: startup_r.bottom },
        &text_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    draw_toggle(ctx, &D2D_RECT_F {
        left: startup_r.right - 60.0,
        top: startup_r.top + 12.0,
        right: startup_r.right - 16.0,
        bottom: startup_r.bottom - 12.0,
    }, s.startup_enabled, &pal)?;
    s.hits.toggle_startup = startup_r;
    y += 60.0;

    // Record total content height for scrollbar/clamp math.
    s.content_h = y;

    // End scroll viewport
    ctx.SetTransform(&windows_numerics::Matrix3x2 {
        M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0,
    });
    ctx.PopAxisAlignedClip();

    // Scrollbar (thin, right-edge of viewport)
    if s.content_h > viewport_h {
        let max_scroll = (s.content_h - viewport_h).max(0.0);
        s.scroll_y = s.scroll_y.clamp(0.0, max_scroll);
        let track_x = w - 6.0;
        let track_top = viewport_top + 4.0;
        let track_bottom = viewport_bottom - 4.0;
        let track_rect = D2D_RECT_F { left: track_x, top: track_top, right: track_x + 3.0, bottom: track_bottom };
        let _ = ctx.CreateSolidColorBrush(&pal.stroke, None).map(|b| {
            ctx.FillRectangle(&track_rect, &b);
        });
        let bar_h = ((viewport_h / s.content_h) * (track_bottom - track_top)).max(24.0);
        let bar_pos = (s.scroll_y / max_scroll) * (track_bottom - track_top - bar_h);
        let bar_rect = D2D_RECT_F {
            left: track_x,
            top: track_top + bar_pos,
            right: track_x + 3.0,
            bottom: track_top + bar_pos + bar_h,
        };
        let _ = ctx.CreateSolidColorBrush(&pal.text_dim, None).map(|b| {
            ctx.FillRectangle(&bar_rect, &b);
        });
    } else {
        s.scroll_y = 0.0;
    }

    // Subtle separator above the footer
    let sep_y = viewport_bottom;
    let sep_rect = D2D_RECT_F { left: 0.0, top: sep_y, right: w, bottom: sep_y + 1.0 };
    ctx.FillRectangle(&sep_rect, &stroke_brush);

    // Footer with sticky buttons
    let btn_h = 36.0;
    let btn_w = 100.0;
    let by = h - (FOOTER_H - btn_h) / 2.0 - btn_h;
    let preview_r = D2D_RECT_F { left: pad, top: by, right: pad + btn_w, bottom: by + btn_h };
    draw_button(ctx, &s.dwrite_factory, &s.body_format, &preview_r, "Preview", false, &pal, &text_brush, &accent_brush)?;
    s.hits.btn_preview = preview_r;

    let close_r = D2D_RECT_F { left: w - pad - btn_w, top: by, right: w - pad, bottom: by + btn_h };
    draw_button(ctx, &s.dwrite_factory, &s.body_format, &close_r, "Close", false, &pal, &text_brush, &accent_brush)?;
    s.hits.btn_close = close_r;

    let save_r = D2D_RECT_F { left: close_r.left - 12.0 - btn_w, top: by, right: close_r.left - 12.0, bottom: by + btn_h };
    let flash = s.saved_flash > 0;
    let save_label = if flash { "Saved" } else { "Save" };
    draw_button(ctx, &s.dwrite_factory, &s.body_format, &save_r, save_label, true, &pal, &accent_brush, &accent_brush)?;
    s.hits.btn_save = save_r;
    if s.saved_flash > 0 { s.saved_flash -= 1; }

    ctx.EndDraw(None, None)?;
    s.swapchain.Present(1, DXGI_PRESENT::default()).ok()?;
    s.dcomp_device.Commit()?;
    Ok(())
}

unsafe fn draw_anchor_cell(
    ctx: &ID2D1DeviceContext,
    dw: &IDWriteFactory,
    icon_fmt: &IDWriteTextFormat,
    small_fmt: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    glyph: &str,
    label: &str,
    selected: bool,
    pal: &Pal,
) -> Result<()> {
    let fill_color = if selected { pal.accent } else { pal.panel };
    let text_color = if selected { pal.accent_text } else { pal.text };
    let fill: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&fill_color, None)?;
    let text_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&text_color, None)?;
    let stroke: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.stroke, None)?;
    let rr = D2D1_ROUNDED_RECT { rect: *rect, radiusX: 8.0, radiusY: 8.0 };
    ctx.FillRoundedRectangle(&rr, &fill);
    ctx.DrawRoundedRectangle(&rr, &stroke, 1.0, None);

    let icon_rect = D2D_RECT_F {
        left: rect.left,
        top: rect.top + 4.0,
        right: rect.right,
        bottom: rect.top + 30.0,
    };
    draw_text(ctx, dw, glyph, icon_fmt, &icon_rect, &text_brush, DWRITE_TEXT_ALIGNMENT_CENTER)?;

    let label_rect = D2D_RECT_F {
        left: rect.left,
        top: rect.top + 30.0,
        right: rect.right,
        bottom: rect.bottom - 4.0,
    };
    draw_text(ctx, dw, label, small_fmt, &label_rect, &text_brush, DWRITE_TEXT_ALIGNMENT_CENTER)?;
    Ok(())
}

unsafe fn draw_panel(ctx: &ID2D1DeviceContext, rect: &D2D_RECT_F, selected: bool, pal: &Pal) -> Result<()> {
    let fill_color = if selected { pal.accent } else { pal.panel };
    let fill: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&fill_color, None)?;
    let stroke: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.stroke, None)?;
    let rr = D2D1_ROUNDED_RECT { rect: *rect, radiusX: 8.0, radiusY: 8.0 };
    ctx.FillRoundedRectangle(&rr, &fill);
    ctx.DrawRoundedRectangle(&rr, &stroke, 1.0, None);
    Ok(())
}

unsafe fn draw_panel_static(
    ctx: &ID2D1DeviceContext,
    rect: &D2D_RECT_F,
    fill: &ID2D1SolidColorBrush,
    stroke: &ID2D1SolidColorBrush,
) -> Result<()> {
    let rr = D2D1_ROUNDED_RECT { rect: *rect, radiusX: 8.0, radiusY: 8.0 };
    ctx.FillRoundedRectangle(&rr, fill);
    ctx.DrawRoundedRectangle(&rr, stroke, 1.0, None);
    Ok(())
}

unsafe fn draw_radio(ctx: &ID2D1DeviceContext, cx: f32, cy: f32, selected: bool, pal: &Pal) -> Result<()> {
    let ring_color = if selected { pal.accent_text } else { pal.text_dim };
    let ring: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&ring_color, None)?;
    let outer = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: 8.0, radiusY: 8.0 };
    ctx.DrawEllipse(&outer, &ring, 1.5, None);
    if selected {
        let dot: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.accent_text, None)?;
        let inner = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: 4.0, radiusY: 4.0 };
        ctx.FillEllipse(&inner, &dot);
    }
    Ok(())
}

unsafe fn draw_toggle(ctx: &ID2D1DeviceContext, rect: &D2D_RECT_F, on: bool, pal: &Pal) -> Result<()> {
    let bg_color = if on { pal.accent } else { pal.field_bg };
    let bg: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&bg_color, None)?;
    let h = rect.bottom - rect.top;
    let rr = D2D1_ROUNDED_RECT { rect: *rect, radiusX: h / 2.0, radiusY: h / 2.0 };
    ctx.FillRoundedRectangle(&rr, &bg);
    let stroke: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.field_border, None)?;
    ctx.DrawRoundedRectangle(&rr, &stroke, 1.0, None);

    let knob_color = if on { pal.accent_text } else { pal.text };
    let knob: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&knob_color, None)?;
    let knob_r = h / 2.0 - 4.0;
    let knob_cy = (rect.top + rect.bottom) / 2.0;
    let knob_cx = if on { rect.right - h / 2.0 } else { rect.left + h / 2.0 };
    let e = D2D1_ELLIPSE { point: Vector2 { X: knob_cx, Y: knob_cy }, radiusX: knob_r, radiusY: knob_r };
    ctx.FillEllipse(&e, &knob);
    Ok(())
}

unsafe fn draw_field(
    ctx: &ID2D1DeviceContext,
    dw: &IDWriteFactory,
    fmt: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    value: &str,
    focused: bool,
    pal: &Pal,
    text_brush: &ID2D1SolidColorBrush,
) -> Result<()> {
    let fill: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.field_bg, None)?;
    let border_color = if focused { pal.field_focus } else { pal.field_border };
    let border: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&border_color, None)?;
    let rr = D2D1_ROUNDED_RECT { rect: *rect, radiusX: 4.0, radiusY: 4.0 };
    ctx.FillRoundedRectangle(&rr, &fill);
    ctx.DrawRoundedRectangle(&rr, &border, if focused { 2.0 } else { 1.0 }, None);

    let inner = D2D_RECT_F {
        left: rect.left + 8.0,
        top: rect.top,
        right: rect.right - 8.0,
        bottom: rect.bottom,
    };
    draw_text(ctx, dw, value, fmt, &inner, text_brush, DWRITE_TEXT_ALIGNMENT_LEADING)?;
    Ok(())
}

unsafe fn draw_button(
    ctx: &ID2D1DeviceContext,
    dw: &IDWriteFactory,
    fmt: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    label: &str,
    primary: bool,
    pal: &Pal,
    primary_brush: &ID2D1SolidColorBrush,
    accent_brush: &ID2D1SolidColorBrush,
) -> Result<()> {
    let _ = primary_brush;
    let _ = accent_brush;
    let bg_color = if primary { pal.accent } else { pal.panel };
    let text_color = if primary { pal.accent_text } else { pal.text };
    let bg: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&bg_color, None)?;
    let text_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&text_color, None)?;
    let stroke: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&pal.stroke, None)?;
    let rr = D2D1_ROUNDED_RECT { rect: *rect, radiusX: 6.0, radiusY: 6.0 };
    ctx.FillRoundedRectangle(&rr, &bg);
    ctx.DrawRoundedRectangle(&rr, &stroke, 1.0, None);
    draw_text(ctx, dw, label, fmt, rect, &text_brush, DWRITE_TEXT_ALIGNMENT_CENTER)?;
    Ok(())
}

unsafe fn draw_text(
    ctx: &ID2D1DeviceContext,
    factory: &IDWriteFactory,
    text: &str,
    format: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    brush: &ID2D1SolidColorBrush,
    align: DWRITE_TEXT_ALIGNMENT,
) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let layout = factory.CreateTextLayout(
        &wide,
        format,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )?;
    layout.SetTextAlignment(align)?;
    layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
    layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    ctx.DrawTextLayout(
        Vector2 { X: rect.left, Y: rect.top },
        &layout,
        brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
    Ok(())
}

fn hit(x: f32, y: f32, r: &D2D_RECT_F) -> bool {
    x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

unsafe fn handle_click(hwnd: HWND, x: f32, y: f32) {
    let mut request_paint = true;
    let mut do_preview = false;
    let mut do_save = false;
    let mut do_close = false;
    let mut toggle_startup_now = false;

    let viewport_top = TITLE_H;
    let viewport_bottom = client_size().1 as f32 - FOOTER_H;

    STATE.with(|cell| {
        if let Some(s) = cell.borrow_mut().as_mut() {
            // Footer click: window-coord rects
            if y >= viewport_bottom {
                if hit(x, y, &s.hits.btn_preview) { do_preview = true; }
                if hit(x, y, &s.hits.btn_save) { do_save = true; }
                if hit(x, y, &s.hits.btn_close) { do_close = true; }
                return;
            }

            // Inside the scroll viewport: convert click into content coordinates
            if y < viewport_top { return; }
            let cy = y - viewport_top + s.scroll_y;

            for i in 0..7 {
                if hit(x, cy, &s.hits.anchors[i]) {
                    s.cfg.anchor = index_to_anchor(i);
                }
            }
            if hit(x, cy, &s.hits.field_mx) {
                s.focus = Field::Mx;
            } else if hit(x, cy, &s.hits.field_my) {
                s.focus = Field::My;
            } else if hit(x, cy, &s.hits.field_cx) {
                s.focus = Field::Cx;
                s.cfg.anchor = Anchor::Custom;
            } else if hit(x, cy, &s.hits.field_cy) {
                s.focus = Field::Cy;
                s.cfg.anchor = Anchor::Custom;
            } else if hit(x, cy, &s.hits.field_hold) {
                s.focus = Field::Hold;
            } else {
                s.focus = Field::None;
            }
            if hit(x, cy, &s.hits.toggle_compact) {
                s.cfg.compact = !s.cfg.compact;
            }
            if hit(x, cy, &s.hits.toggle_startup) {
                toggle_startup_now = true;
            }
        } else {
            request_paint = false;
        }
    });

    if toggle_startup_now {
        let now_enabled = startup::is_enabled();
        let res = if now_enabled { startup::disable() } else { startup::enable() };
        STATE.with(|cell| {
            if let Some(s) = cell.borrow_mut().as_mut() {
                if res.is_ok() {
                    s.startup_enabled = !now_enabled;
                }
            }
        });
    }
    if do_preview {
        let cfg = STATE.with(|cell| cell.borrow().as_ref().map(|s| s.cfg).unwrap_or_default());
        config::set(cfg);
        flyout::show();
    }
    if do_save {
        let cfg = STATE.with(|cell| cell.borrow().as_ref().map(|s| s.cfg).unwrap_or_default());
        config::set(cfg);
        STATE.with(|cell| {
            if let Some(s) = cell.borrow_mut().as_mut() {
                s.saved_flash = 60; // ~1 second at 60fps repaints
            }
        });
    }
    if do_close {
        let _ = DestroyWindow(hwnd);
        return;
    }
    if request_paint {
        paint(hwnd);
    }
}

unsafe fn handle_char(hwnd: HWND, ch: u32) {
    let c = char::from_u32(ch).unwrap_or('\0');
    STATE.with(|cell| {
        if let Some(s) = cell.borrow_mut().as_mut() {
            if s.focus == Field::None {
                return;
            }
            let cur = match s.focus {
                Field::Mx => s.cfg.margin_x.to_string(),
                Field::My => s.cfg.margin_y.to_string(),
                Field::Cx => s.cfg.custom_x.to_string(),
                Field::Cy => s.cfg.custom_y.to_string(),
                Field::Hold => s.cfg.hold_ms.to_string(),
                Field::None => return,
            };
            let mut buf = cur;
            if c == '\u{8}' {
                buf.pop();
            } else if c.is_ascii_digit() || (c == '-' && buf.is_empty()) {
                buf.push(c);
            } else {
                return;
            }
            match s.focus {
                Field::Mx => s.cfg.margin_x = buf.parse().unwrap_or(0),
                Field::My => s.cfg.margin_y = buf.parse().unwrap_or(0),
                Field::Cx => s.cfg.custom_x = buf.parse().unwrap_or(0),
                Field::Cy => s.cfg.custom_y = buf.parse().unwrap_or(0),
                Field::Hold => {
                    s.cfg.hold_ms = buf.parse::<u32>().unwrap_or(500).max(500).min(30_000);
                }
                Field::None => {}
            }
        }
    });
    paint(hwnd);
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as i16 as f32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                handle_click(hwnd, x, y);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = (wparam.0 >> 16) as i16 as f32; // signed
                STATE.with(|cell| {
                    if let Some(s) = cell.borrow_mut().as_mut() {
                        // 3 lines per notch, ~40 px per line
                        s.scroll_y -= delta / 120.0 * 60.0;
                        let viewport_h = client_size().1 as f32 - TITLE_H - FOOTER_H;
                        let max = (s.content_h - viewport_h).max(0.0);
                        s.scroll_y = s.scroll_y.clamp(0.0, max);
                    }
                });
                paint(hwnd);
                LRESULT(0)
            }
            WM_CHAR => {
                handle_char(hwnd, wparam.0 as u32);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u32;
                if vk == VK_ESCAPE.0 as u32 {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let _ = SetCursor(Some(LoadCursorW(None, IDC_ARROW).unwrap_or_default()));
                LRESULT(1)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let _ = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);
                paint(hwnd);
                let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                WINDOW.store(0, Ordering::Release);
                STATE.with(|cell| *cell.borrow_mut() = None);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
