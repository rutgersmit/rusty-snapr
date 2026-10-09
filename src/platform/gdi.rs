//! Owned GDI objects: 32-bit DIB sections with direct pixel access, memory DCs and fonts.

use std::ffi::c_void;

use snapr_core::editor::Bitmap;
use snapr_core::geometry::IRect;
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    COLORONCOLOR, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET,
    DEFAULT_PITCH, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FW_BOLD,
    FW_NORMAL, GdiFlush, GetTextExtentPoint32W, HALFTONE, HBITMAP, HDC, HFONT, HGDIOBJ,
    OUT_DEFAULT_PRECIS, SRCCOPY, SelectObject, SetBkMode, SetBrushOrgEx, SetStretchBltMode,
    SetTextColor, StretchDIBits, TRANSPARENT,
};
use windows::core::{PCWSTR, Result};

use super::wide;

/// A top-down 32-bit BGRA bitmap whose pixels can be read and written directly.
pub struct Dib {
    handle: HBITMAP,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Dib {
    pub fn new(width: i32, height: i32) -> Result<Dib> {
        assert!(width > 0 && height > 0);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negative: rows top to bottom
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` describes a valid 32-bit DIB; `bits` receives the pixel pointer.
        let handle = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)? };
        if bits.is_null() {
            // SAFETY: the bitmap was just created by us.
            unsafe {
                let _ = DeleteObject(handle.into());
            }
            return Err(windows::core::Error::from_thread());
        }
        Ok(Dib {
            handle,
            bits: bits.cast(),
            width,
            height,
        })
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn handle(&self) -> HBITMAP {
        self.handle
    }

    fn len(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// The pixels (BGRA, rows top to bottom, no padding). Flushes pending GDI drawing first.
    pub fn pixels(&self) -> &[u8] {
        // SAFETY: GdiFlush makes earlier GDI drawing visible; the DIB owns `len` bytes at `bits`
        // for as long as it lives.
        unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts(self.bits, self.len())
        }
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        // SAFETY: as above; `&mut self` guarantees exclusive access from Rust.
        unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts_mut(self.bits, self.len())
        }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: we own the bitmap; it is no longer selected into a DC (MemDc restores the old one).
        unsafe {
            let _ = DeleteObject(self.handle.into());
        }
    }
}

/// A memory device context with a bitmap selected into it. Drop it before the bitmap.
pub struct MemDc {
    hdc: HDC,
    previous: HGDIOBJ,
}

impl MemDc {
    pub fn new(bitmap: HBITMAP) -> Result<MemDc> {
        // SAFETY: creates a DC compatible with the screen and selects our bitmap into it.
        unsafe {
            let hdc = CreateCompatibleDC(None);
            if hdc.is_invalid() {
                return Err(windows::core::Error::from_thread());
            }
            let previous = SelectObject(hdc, bitmap.into());
            Ok(MemDc { hdc, previous })
        }
    }

    pub fn hdc(&self) -> HDC {
        self.hdc
    }
}

impl Drop for MemDc {
    fn drop(&mut self) {
        // SAFETY: restores the DC's original bitmap, then deletes the DC we created.
        unsafe {
            SelectObject(self.hdc, self.previous);
            let _ = DeleteDC(self.hdc);
        }
    }
}

/// A Segoe UI font of a given pixel height.
pub struct Font(HFONT);

impl Font {
    pub fn segoe_ui(pixel_height: i32) -> Font {
        Self::segoe_ui_weight(pixel_height, FW_NORMAL.0 as i32)
    }

    pub fn segoe_ui_bold(pixel_height: i32) -> Font {
        Self::segoe_ui_weight(pixel_height, FW_BOLD.0 as i32)
    }

    fn segoe_ui_weight(pixel_height: i32, weight: i32) -> Font {
        let face = wide("Segoe UI");
        // SAFETY: plain font creation; `face` outlives the call. Falls back to a default font if
        // Segoe UI is missing.
        let font = unsafe {
            CreateFontW(
                -pixel_height.max(1),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(face.as_ptr()),
            )
        };
        Font(font)
    }

    pub fn handle(&self) -> HFONT {
        self.0
    }

    /// Size of `text` in this font, in pixels.
    pub fn measure(&self, hdc: HDC, text: &str) -> (i32, i32) {
        let text: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        // SAFETY: the font is selected only for the measurement and then restored.
        unsafe {
            let previous = SelectObject(hdc, self.0.into());
            let _ = GetTextExtentPoint32W(hdc, &text, &mut size);
            SelectObject(hdc, previous);
        }
        (size.cx, size.cy)
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: we own the font and it is not selected into any DC anymore.
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

/// Copies a `width`×`height` block of pixels from `src` at (`src_x`, `src_y`) to `dst` at
/// (`dst_x`, `dst_y`).
#[allow(clippy::too_many_arguments)]
pub fn blit(
    dst: HDC,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src: HDC,
    src_x: i32,
    src_y: i32,
) {
    if width <= 0 || height <= 0 {
        return;
    }
    // SAFETY: both DCs are valid for the duration of the call; GDI clips to the bitmaps.
    unsafe {
        let _ = BitBlt(
            dst,
            dst_x,
            dst_y,
            width,
            height,
            Some(src),
            src_x,
            src_y,
            SRCCOPY,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Draws one line of text centred in `rect`, with a transparent background.
pub fn draw_text_centered(hdc: HDC, font: &Font, text: &str, rect: IRect, rgb: (u8, u8, u8)) {
    draw_text(hdc, font, text, rect, rgb, Align::Center);
}

/// Draws one line of text, vertically centred in `rect`, cut off with "…" if it doesn't fit.
pub fn draw_text(hdc: HDC, font: &Font, text: &str, rect: IRect, rgb: (u8, u8, u8), align: Align) {
    let mut text: Vec<u16> = text.encode_utf16().collect();
    let mut rect = RECT {
        left: rect.x,
        top: rect.y,
        right: rect.right(),
        bottom: rect.bottom(),
    };
    let color = COLORREF(rgb.0 as u32 | (rgb.1 as u32) << 8 | (rgb.2 as u32) << 16);
    let horizontal: DRAW_TEXT_FORMAT = match align {
        Align::Left => DT_LEFT,
        Align::Center => DT_CENTER,
        Align::Right => DT_RIGHT,
    };
    // SAFETY: the font is selected only while drawing and then restored.
    unsafe {
        let previous = SelectObject(hdc, font.handle().into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, color);
        DrawTextW(
            hdc,
            &mut text,
            &mut rect,
            horizontal | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
        SelectObject(hdc, previous);
    }
}

/// Draws a bitmap into `dest`, scaling it if the sizes differ (smoothly when shrinking).
pub fn draw_bitmap(hdc: HDC, dest: IRect, image: &Bitmap) {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width() as i32,
            biHeight: -(image.height() as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let scaled = dest.width != image.width() as i32 || dest.height != image.height() as i32;
    // SAFETY: `info` describes `image`'s pixel buffer exactly; GDI only reads from it.
    unsafe {
        if scaled {
            SetStretchBltMode(hdc, HALFTONE);
            let _ = SetBrushOrgEx(hdc, 0, 0, None);
        } else {
            SetStretchBltMode(hdc, COLORONCOLOR);
        }
        StretchDIBits(
            hdc,
            dest.x,
            dest.y,
            dest.width,
            dest.height,
            0,
            0,
            image.width() as i32,
            image.height() as i32,
            Some(image.pixels().as_ptr().cast()),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}
