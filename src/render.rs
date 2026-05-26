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
            Imaging::*,
        },
        System::Com::*,
    },
};

use windows_numerics::{Matrix3x2, Vector2};

use crate::flyout::{FLYOUT_H, FLYOUT_H_COMPACT, FLYOUT_H_EXPANDED, FLYOUT_W, FLYOUT_W_COMPACT};

pub struct Renderer {
    d2d_factory: ID2D1Factory1,
    dwrite_factory: IDWriteFactory,
    wic_factory: IWICImagingFactory,
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
    artist_format: IDWriteTextFormat,
    time_format: IDWriteTextFormat,
    icon_format: IDWriteTextFormat,

    album_bitmap: Option<ID2D1Bitmap>,
    album_key: Option<String>,
}

#[derive(Clone, Copy, Default)]
pub struct HitRegions {
    pub prev: D2D_RECT_F,
    pub play_pause: D2D_RECT_F,
    pub next: D2D_RECT_F,
    pub seek_track: D2D_RECT_F,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: D2D1_COLOR_F,
    pub text: D2D1_COLOR_F,
    pub text_dim: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub stroke: D2D1_COLOR_F,
    pub track: D2D1_COLOR_F,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: rgba(0x20, 0x20, 0x20, 0xF2),
            text: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            text_dim: rgba(0xFF, 0xFF, 0xFF, 0xB0),
            accent: rgba(0x4C, 0xC2, 0xFF, 0xFF),
            stroke: rgba(0xFF, 0xFF, 0xFF, 0x26),
            track: rgba(0xFF, 0xFF, 0xFF, 0x70),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: rgba(0xF3, 0xF3, 0xF3, 0xF2),
            text: rgba(0x10, 0x10, 0x10, 0xFF),
            text_dim: rgba(0x10, 0x10, 0x10, 0x99),
            accent: rgba(0x00, 0x67, 0xC0, 0xFF),
            stroke: rgba(0x00, 0x00, 0x00, 0x1F),
            track: rgba(0x00, 0x00, 0x00, 0x33),
        }
    }
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}

impl Renderer {
    pub unsafe fn new(hwnd: HWND) -> Result<Self> {
        let d2d_factory: ID2D1Factory1 = D2D1CreateFactory::<ID2D1Factory1>(
            D2D1_FACTORY_TYPE_SINGLE_THREADED,
            Some(&D2D1_FACTORY_OPTIONS::default()),
        )?;

        let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

        let wic_factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

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

        let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS::default())?;

        let swap_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: FLYOUT_W as u32,
            Height: FLYOUT_H_EXPANDED as u32,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: BOOL(0),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
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

        let target_bitmap = Self::create_target_bitmap(&swapchain, &d2d_context)?;
        d2d_context.SetTarget(&target_bitmap);

        let title_format = create_text_format(&dwrite_factory, "Segoe UI Variable Display", 14.0, true)?;
        let artist_format = create_text_format(&dwrite_factory, "Segoe UI Variable Display", 13.0, false)?;
        let time_format = create_text_format(&dwrite_factory, "Segoe UI Variable Small", 11.0, false)?;
        let icon_format = create_text_format(&dwrite_factory, "Segoe Fluent Icons", 14.0, false)?;

        Ok(Self {
            d2d_factory,
            dwrite_factory,
            wic_factory,
            d2d_device,
            d2d_context,
            swapchain,
            dcomp_device,
            dcomp_target,
            dcomp_visual,
            target_bitmap,
            title_format,
            artist_format,
            time_format,
            icon_format,
            album_bitmap: None,
            album_key: None,
        })
    }

    unsafe fn create_target_bitmap(
        swapchain: &IDXGISwapChain1,
        ctx: &ID2D1DeviceContext,
    ) -> Result<ID2D1Bitmap1> {
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
        let bmp = ctx.CreateBitmapFromDxgiSurface(&back, Some(&props))?;
        Ok(bmp)
    }

    pub fn set_album_bitmap_from_bytes(&mut self, key: &str, bytes: &[u8]) {
        if self.album_key.as_deref() == Some(key) {
            return;
        }
        if let Ok(bmp) = unsafe { self.decode_image(bytes) } {
            self.album_bitmap = Some(bmp);
            self.album_key = Some(key.to_string());
        }
    }

    pub fn clear_album(&mut self) {
        self.album_bitmap = None;
        self.album_key = None;
    }

    unsafe fn decode_image(&self, bytes: &[u8]) -> Result<ID2D1Bitmap> {
        let stream = self.wic_factory.CreateStream()?;
        stream.InitializeFromMemory(std::slice::from_raw_parts_mut(
            bytes.as_ptr() as *mut u8,
            bytes.len(),
        ))?;
        let decoder = self.wic_factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnLoad,
        )?;
        let frame = decoder.GetFrame(0)?;
        let converter = self.wic_factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        )?;
        let bmp = self
            .d2d_context
            .CreateBitmapFromWicBitmap(&converter, None)?;
        let generic: ID2D1Bitmap = bmp.cast()?;
        Ok(generic)
    }

    pub unsafe fn render(
        &self,
        theme: &Theme,
        track_title: &str,
        track_artist: &str,
        playing: bool,
        position_secs: f64,
        duration_secs: f64,
        seekbar_enabled: bool,
        compact: bool,
    ) -> Result<HitRegions> {
        if compact {
            return self.render_compact(theme, track_title, track_artist);
        }

        let ctx = &self.d2d_context;
        ctx.BeginDraw();
        ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));

        let w = FLYOUT_W as f32;
        let h = (if seekbar_enabled { FLYOUT_H + 36 } else { FLYOUT_H }) as f32;

        // Rounded background
        let bg_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.bg, None)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: h },
            radiusX: 8.0,
            radiusY: 8.0,
        };
        ctx.FillRoundedRectangle(&rr, &bg_brush);

        let stroke_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.stroke, None)?;
        ctx.DrawRoundedRectangle(&rr, &stroke_brush, 1.0, None);

        // Album art
        let art_rect = D2D_RECT_F {
            left: 12.0,
            top: 12.0,
            right: 12.0 + 78.0,
            bottom: 12.0 + 78.0,
        };
        let art_clip = D2D1_ROUNDED_RECT { rect: art_rect, radiusX: 6.0, radiusY: 6.0 };
        if let Some(bmp) = &self.album_bitmap {
            ctx.FillRoundedRectangle(&art_clip, &stroke_brush);
            let layer = ctx.CreateLayer(None)?;
            let geom: ID2D1RoundedRectangleGeometry =
                self.d2d_factory.CreateRoundedRectangleGeometry(&art_clip)?;
            let layer_params = D2D1_LAYER_PARAMETERS1 {
                contentBounds: art_rect,
                geometricMask: std::mem::ManuallyDrop::new(Some(geom.cast::<ID2D1Geometry>()?)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            ctx.PushLayer(&layer_params, &layer);
            ctx.DrawBitmap(
                bmp,
                Some(&art_rect),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            );
            ctx.PopLayer();
        } else {
            let placeholder_brush: ID2D1SolidColorBrush =
                ctx.CreateSolidColorBrush(&rgba(0x30, 0x30, 0x30, 0xFF), None)?;
            ctx.FillRoundedRectangle(&art_clip, &placeholder_brush);
            ctx.DrawRoundedRectangle(&art_clip, &stroke_brush, 1.0, None);
            draw_text(
                ctx,
                "\u{E8D6}", // Segoe Fluent: MusicNote
                &self.icon_format,
                &art_rect,
                &theme.text_dim,
                self.dwrite_factory.clone(),
            )?;
        }

        // Title + artist
        let text_left = 12.0 + 78.0 + 12.0;
        let title_rect = D2D_RECT_F {
            left: text_left,
            top: 14.0,
            right: w - 12.0,
            bottom: 36.0,
        };
        let artist_rect = D2D_RECT_F {
            left: text_left,
            top: 34.0,
            right: w - 12.0,
            bottom: 56.0,
        };
        let title_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.text, None)?;
        let dim_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.text_dim, None)?;

        let title_display = if track_title.is_empty() {
            "Not Playing"
        } else {
            track_title
        };
        draw_text_left_aligned(
            ctx,
            title_display,
            &self.title_format,
            &title_rect,
            &title_brush,
            self.dwrite_factory.clone(),
        )?;
        draw_text_left_aligned(
            ctx,
            track_artist,
            &self.artist_format,
            &artist_rect,
            &dim_brush,
            self.dwrite_factory.clone(),
        )?;

        // Transport buttons (right side of content row)
        let btn_y = 60.0;
        let btn_h = 32.0;
        let big_btn = 36.0;
        let small_btn = 28.0;

        let next_x = w - 12.0 - small_btn;
        let play_x = next_x - 6.0 - big_btn;
        let prev_x = play_x - 6.0 - small_btn;

        let prev_rect = D2D_RECT_F {
            left: prev_x,
            top: btn_y + (big_btn - btn_h) / 2.0,
            right: prev_x + small_btn,
            bottom: btn_y + (big_btn - btn_h) / 2.0 + btn_h,
        };
        let play_rect = D2D_RECT_F {
            left: play_x,
            top: btn_y,
            right: play_x + big_btn,
            bottom: btn_y + big_btn,
        };
        let next_rect = D2D_RECT_F {
            left: next_x,
            top: btn_y + (big_btn - btn_h) / 2.0,
            right: next_x + small_btn,
            bottom: btn_y + (big_btn - btn_h) / 2.0 + btn_h,
        };

        // Transparent backgrounds for prev/next, accent-filled for play-pause
        let accent_brush: ID2D1SolidColorBrush =
            ctx.CreateSolidColorBrush(&theme.accent, None)?;
        let play_rr = D2D1_ROUNDED_RECT {
            rect: play_rect,
            radiusX: big_btn / 2.0,
            radiusY: big_btn / 2.0,
        };
        ctx.FillRoundedRectangle(&play_rr, &accent_brush);

        let play_icon_color = rgba(0x00, 0x00, 0x00, 0xE6);
        let play_icon_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&play_icon_color, None)?;

        draw_text(
            ctx,
            "\u{E892}", // Previous
            &self.icon_format,
            &prev_rect,
            &theme.text,
            self.dwrite_factory.clone(),
        )?;
        let pp_glyph = if playing { "\u{E769}" } else { "\u{E768}" }; // Pause / Play
        draw_text(
            ctx,
            pp_glyph,
            &self.icon_format,
            &play_rect,
            &play_icon_color,
            self.dwrite_factory.clone(),
        )?;
        let _ = play_icon_brush;
        draw_text(
            ctx,
            "\u{E893}", // Next
            &self.icon_format,
            &next_rect,
            &theme.text,
            self.dwrite_factory.clone(),
        )?;

        let mut hits = HitRegions {
            prev: prev_rect,
            play_pause: play_rect,
            next: next_rect,
            seek_track: D2D_RECT_F::default(),
        };

        // Seek bar
        if seekbar_enabled {
            let seek_y = FLYOUT_H as f32 + 6.0;
            let time_w = 46.0;
            let track_left = 16.0 + time_w;
            let track_right = w - 16.0 - time_w;
            let track_rect = D2D_RECT_F {
                left: track_left,
                top: seek_y + 8.0,
                right: track_right,
                bottom: seek_y + 14.0,
            };
            let track_rr = D2D1_ROUNDED_RECT {
                rect: track_rect,
                radiusX: 3.0,
                radiusY: 3.0,
            };
            let track_bg: ID2D1SolidColorBrush =
                ctx.CreateSolidColorBrush(&theme.track, None)?;
            ctx.FillRoundedRectangle(&track_rr, &track_bg);

            let progress = if duration_secs > 0.0 {
                (position_secs / duration_secs).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            if progress > 0.0 {
                let fill_rect = D2D_RECT_F {
                    left: track_rect.left,
                    top: track_rect.top,
                    right: track_rect.left + (track_rect.right - track_rect.left) * progress,
                    bottom: track_rect.bottom,
                };
                let fill_rr = D2D1_ROUNDED_RECT {
                    rect: fill_rect,
                    radiusX: 2.0,
                    radiusY: 2.0,
                };
                ctx.FillRoundedRectangle(&fill_rr, &accent_brush);

                let thumb_cx = fill_rect.right;
                let thumb_cy = (track_rect.top + track_rect.bottom) / 2.0;
                let thumb_ellipse = D2D1_ELLIPSE {
                    point: Vector2 { X: thumb_cx, Y: thumb_cy },
                    radiusX: 6.0,
                    radiusY: 6.0,
                };
                ctx.FillEllipse(&thumb_ellipse, &accent_brush);
            }

            let time_left_rect = D2D_RECT_F {
                left: 12.0,
                top: seek_y,
                right: track_left - 4.0,
                bottom: seek_y + 24.0,
            };
            let time_right_rect = D2D_RECT_F {
                left: track_right + 4.0,
                top: seek_y,
                right: w - 12.0,
                bottom: seek_y + 24.0,
            };
            draw_text_left_aligned(
                ctx,
                &format_time(position_secs),
                &self.time_format,
                &time_left_rect,
                &dim_brush,
                self.dwrite_factory.clone(),
            )?;
            draw_text_right_aligned(
                ctx,
                &format_time(duration_secs),
                &self.time_format,
                &time_right_rect,
                &dim_brush,
                self.dwrite_factory.clone(),
            )?;

            hits.seek_track = D2D_RECT_F {
                left: track_rect.left,
                top: track_rect.top - 10.0,
                right: track_rect.right,
                bottom: track_rect.bottom + 10.0,
            };
        }

        ctx.EndDraw(None, None)?;
        self.swapchain.Present(1, DXGI_PRESENT::default()).ok()?;
        self.dcomp_device.Commit()?;
        Ok(hits)
    }

    unsafe fn render_compact(
        &self,
        theme: &Theme,
        track_title: &str,
        track_artist: &str,
    ) -> Result<HitRegions> {
        let ctx = &self.d2d_context;
        ctx.BeginDraw();
        ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));

        let w = FLYOUT_W_COMPACT as f32;
        let h = FLYOUT_H_COMPACT as f32;

        // Rounded background
        let bg_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.bg, None)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: h },
            radiusX: 8.0,
            radiusY: 8.0,
        };
        ctx.FillRoundedRectangle(&rr, &bg_brush);

        let stroke_brush: ID2D1SolidColorBrush =
            ctx.CreateSolidColorBrush(&theme.stroke, None)?;
        ctx.DrawRoundedRectangle(&rr, &stroke_brush, 1.0, None);

        // Album art (44x44)
        let art_size = 44.0;
        let art_rect = D2D_RECT_F {
            left: 10.0,
            top: 10.0,
            right: 10.0 + art_size,
            bottom: 10.0 + art_size,
        };
        let art_clip = D2D1_ROUNDED_RECT { rect: art_rect, radiusX: 5.0, radiusY: 5.0 };
        if let Some(bmp) = &self.album_bitmap {
            ctx.FillRoundedRectangle(&art_clip, &stroke_brush);
            let layer = ctx.CreateLayer(None)?;
            let geom: ID2D1RoundedRectangleGeometry =
                self.d2d_factory.CreateRoundedRectangleGeometry(&art_clip)?;
            let layer_params = D2D1_LAYER_PARAMETERS1 {
                contentBounds: art_rect,
                geometricMask: std::mem::ManuallyDrop::new(Some(geom.cast::<ID2D1Geometry>()?)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            ctx.PushLayer(&layer_params, &layer);
            ctx.DrawBitmap(
                bmp,
                Some(&art_rect),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            );
            ctx.PopLayer();
        } else {
            let placeholder_brush: ID2D1SolidColorBrush =
                ctx.CreateSolidColorBrush(&rgba(0x30, 0x30, 0x30, 0xFF), None)?;
            ctx.FillRoundedRectangle(&art_clip, &placeholder_brush);
            ctx.DrawRoundedRectangle(&art_clip, &stroke_brush, 1.0, None);
            draw_text(
                ctx,
                "\u{E8D6}",
                &self.icon_format,
                &art_rect,
                &theme.text_dim,
                self.dwrite_factory.clone(),
            )?;
        }

        // Title + artist
        let text_left = 10.0 + art_size + 12.0;
        let title_rect = D2D_RECT_F {
            left: text_left,
            top: 9.0,
            right: w - 10.0,
            bottom: 30.0,
        };
        let artist_rect = D2D_RECT_F {
            left: text_left,
            top: 30.0,
            right: w - 10.0,
            bottom: 52.0,
        };
        let title_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.text, None)?;
        let dim_brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(&theme.text_dim, None)?;

        let title_display = if track_title.is_empty() {
            "Not Playing"
        } else {
            track_title
        };
        draw_text_left_aligned(
            ctx,
            title_display,
            &self.title_format,
            &title_rect,
            &title_brush,
            self.dwrite_factory.clone(),
        )?;
        draw_text_left_aligned(
            ctx,
            track_artist,
            &self.artist_format,
            &artist_rect,
            &dim_brush,
            self.dwrite_factory.clone(),
        )?;

        ctx.EndDraw(None, None)?;
        self.swapchain.Present(1, DXGI_PRESENT::default()).ok()?;
        self.dcomp_device.Commit()?;
        Ok(HitRegions::default())
    }
}

fn identity() -> Matrix3x2 {
    Matrix3x2 {
        M11: 1.0,
        M12: 0.0,
        M21: 0.0,
        M22: 1.0,
        M31: 0.0,
        M32: 0.0,
    }
}

fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "-:--".to_string();
    }
    let total = seconds as u64;
    let m = total / 60;
    let s = total % 60;
    format!("{}:{:02}", m, s)
}

unsafe fn create_text_format(
    factory: &IDWriteFactory,
    family: &str,
    size: f32,
    bold: bool,
) -> Result<IDWriteTextFormat> {
    let family_w: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    let locale_w: Vec<u16> = "en-us".encode_utf16().chain(std::iter::once(0)).collect();
    let weight = if bold {
        DWRITE_FONT_WEIGHT_SEMI_BOLD
    } else {
        DWRITE_FONT_WEIGHT_NORMAL
    };
    let format = factory.CreateTextFormat(
        PCWSTR(family_w.as_ptr()),
        None,
        weight,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        size,
        PCWSTR(locale_w.as_ptr()),
    )?;
    Ok(format)
}

unsafe fn draw_text(
    ctx: &ID2D1DeviceContext,
    text: &str,
    format: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    color: &D2D1_COLOR_F,
    factory: IDWriteFactory,
) -> Result<()> {
    let brush: ID2D1SolidColorBrush = ctx.CreateSolidColorBrush(color, None)?;
    let layout = build_layout(
        &factory,
        text,
        format,
        rect.right - rect.left,
        rect.bottom - rect.top,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        false,
    )?;
    ctx.DrawTextLayout(
        Vector2 { X: rect.left, Y: rect.top },
        &layout,
        &brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
    Ok(())
}

unsafe fn draw_text_left_aligned(
    ctx: &ID2D1DeviceContext,
    text: &str,
    format: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    brush: &ID2D1SolidColorBrush,
    factory: IDWriteFactory,
) -> Result<()> {
    let layout = build_layout(
        &factory,
        text,
        format,
        rect.right - rect.left,
        rect.bottom - rect.top,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        true,
    )?;
    ctx.DrawTextLayout(
        Vector2 { X: rect.left, Y: rect.top },
        &layout,
        brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
    Ok(())
}

unsafe fn draw_text_right_aligned(
    ctx: &ID2D1DeviceContext,
    text: &str,
    format: &IDWriteTextFormat,
    rect: &D2D_RECT_F,
    brush: &ID2D1SolidColorBrush,
    factory: IDWriteFactory,
) -> Result<()> {
    let layout = build_layout(
        &factory,
        text,
        format,
        rect.right - rect.left,
        rect.bottom - rect.top,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
        DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        false,
    )?;
    ctx.DrawTextLayout(
        Vector2 { X: rect.left, Y: rect.top },
        &layout,
        brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
    Ok(())
}

unsafe fn build_layout(
    factory: &IDWriteFactory,
    text: &str,
    format: &IDWriteTextFormat,
    max_w: f32,
    max_h: f32,
    text_align: DWRITE_TEXT_ALIGNMENT,
    para_align: DWRITE_PARAGRAPH_ALIGNMENT,
    trim_ellipsis: bool,
) -> Result<IDWriteTextLayout> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let layout = factory.CreateTextLayout(&wide, format, max_w, max_h)?;
    layout.SetTextAlignment(text_align)?;
    layout.SetParagraphAlignment(para_align)?;
    layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    if trim_ellipsis {
        let trim = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        let sign = factory.CreateEllipsisTrimmingSign(format)?;
        layout.SetTrimming(&trim, &sign)?;
    }
    Ok(layout)
}
