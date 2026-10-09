//! Putting an image on the clipboard.

use std::time::Duration;

use snapr_core::editor::{Bitmap, png};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFOHEADER};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_DIB;
use windows::core::w;

/// Puts an image on the clipboard as a device-independent bitmap (what most apps paste) and as
/// PNG (what browsers and chat apps prefer). Retries briefly when another app holds the
/// clipboard open; returns false if it stays busy.
pub fn set_image(owner: HWND, image: &Bitmap) -> bool {
    let dib = dib_bytes(image);
    let png = png::encode(image);

    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        // SAFETY: the clipboard is opened, filled and closed within this block.
        unsafe {
            if OpenClipboard(Some(owner)).is_err() {
                continue;
            }
            let _ = EmptyClipboard();
            let ok = set_data(CF_DIB.0 as u32, &dib);
            let png_format = RegisterClipboardFormatW(w!("PNG"));
            if png_format != 0 {
                set_data(png_format, &png);
            }
            let _ = CloseClipboard();
            return ok;
        }
    }
    false
}

/// Copies `bytes` into global memory and hands it to the open clipboard.
///
/// # Safety
/// The clipboard must be open by this thread.
unsafe fn set_data(format: u32, bytes: &[u8]) -> bool {
    // SAFETY: the memory is allocated with the size we copy; ownership passes to the clipboard on
    // success, otherwise we free it.
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else {
            return false;
        };
        let target = GlobalLock(memory);
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(memory);

        if SetClipboardData(format, Some(HANDLE(memory.0))).is_err() {
            let _ = GlobalFree(Some(HGLOBAL(memory.0)));
            return false;
        }
        true
    }
}

/// A CF_DIB block: BITMAPINFOHEADER followed by bottom-up 32-bit rows.
fn dib_bytes(image: &Bitmap) -> Vec<u8> {
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: image.width() as i32,
        biHeight: image.height() as i32, // positive: rows bottom to top, the most compatible form
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: image.pixels().len() as u32,
        ..Default::default()
    };
    let mut bytes = Vec::with_capacity(size_of::<BITMAPINFOHEADER>() + image.pixels().len());
    // SAFETY: BITMAPINFOHEADER is a plain C struct without padding.
    bytes.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            (&raw const header).cast::<u8>(),
            size_of::<BITMAPINFOHEADER>(),
        )
    });
    for row in image.pixels().chunks_exact(image.stride()).rev() {
        bytes.extend_from_slice(row);
    }
    bytes
}
