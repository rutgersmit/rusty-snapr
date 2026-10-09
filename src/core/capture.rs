//! The logic of the capture overlay, apart from the window itself (port of
//! `CaptureOverlayWindow.xaml.cs`). Coordinates are physical pixels relative to the top-left of
//! the virtual screen; `scale` is the display scale (1.0 at 96 DPI) used for the editor chrome.

use crate::geometry::IRect;

/// A drag shorter than this (in DIPs) in either direction counts as a click.
pub const MIN_DRAG: f64 = 4.0;

/// Opacity of the black layer over everything outside the selection (#73000000).
pub const DIM_ALPHA: u8 = 0x73;

/// What a finished mouse gesture selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// A dragged rectangle.
    Region(IRect),
    /// A plain click: the whole monitor under the cursor.
    Click,
}

/// The rectangle between the drag start and the current mouse position.
pub fn drag_rect(start: (i32, i32), end: (i32, i32)) -> IRect {
    let x = start.0.min(end.0);
    let y = start.1.min(end.1);
    IRect::new(x, y, (start.0 - end.0).abs(), (start.1 - end.1).abs())
}

/// Decides between a dragged region and a click when the mouse button is released.
pub fn finish_drag(start: (i32, i32), end: (i32, i32), scale: f64) -> Gesture {
    let rect = drag_rect(start, end);
    let min = MIN_DRAG * scale;
    if (rect.width as f64) < min || (rect.height as f64) < min {
        Gesture::Click
    } else {
        Gesture::Region(rect)
    }
}

/// The text of the size label, e.g. "640 × 480".
pub fn size_text(selection: IRect) -> String {
    format!("{} × {}", selection.width.max(1), selection.height.max(1))
}

/// Where the size label goes: left-aligned with the selection, just above it, or just below it
/// when there is no room above.
pub fn size_label_origin(selection: IRect, scale: f64) -> (i32, i32) {
    let above = selection.y as f64 >= 28.0 * scale;
    let y = if above {
        selection.y as f64 - 26.0 * scale
    } else {
        selection.bottom() as f64 + 4.0 * scale
    };
    (selection.x, y.round() as i32)
}

/// The hint box: centred horizontally near the top of `monitor`.
pub fn hint_rect(monitor: IRect, width: i32, height: i32, scale: f64) -> IRect {
    let x = monitor.x + (monitor.width - width) / 2;
    let y = monitor.y + (24.0 * scale).round() as i32;
    IRect::new(x, y, width, height)
}

/// Darkens BGRA pixels as if covered by black at [`DIM_ALPHA`]. `src` and `dst` have equal length.
pub fn dim_pixels(src: &[u8], dst: &mut [u8]) {
    assert_eq!(src.len(), dst.len());
    let keep = 255 - DIM_ALPHA as u32;
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        d[0] = ((s[0] as u32 * keep + 127) / 255) as u8;
        d[1] = ((s[1] as u32 * keep + 127) / 255) as u8;
        d[2] = ((s[2] as u32 * keep + 127) / 255) as u8;
        d[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_rect_works_in_any_direction() {
        assert_eq!(drag_rect((10, 20), (4, 50)), IRect::new(4, 20, 6, 30));
    }

    #[test]
    fn short_drags_are_clicks() {
        assert_eq!(finish_drag((10, 10), (12, 40), 1.0), Gesture::Click);
        assert_eq!(finish_drag((10, 10), (10, 10), 1.0), Gesture::Click);
        assert_eq!(
            finish_drag((10, 10), (14, 14), 1.0),
            Gesture::Region(IRect::new(10, 10, 4, 4))
        );
        // At 150% the threshold is 6 physical pixels.
        assert_eq!(finish_drag((10, 10), (15, 30), 1.5), Gesture::Click);
    }

    #[test]
    fn size_label_sits_above_or_below() {
        assert_eq!(
            size_label_origin(IRect::new(50, 100, 10, 10), 1.0),
            (50, 74)
        );
        assert_eq!(size_label_origin(IRect::new(50, 20, 10, 10), 1.0), (50, 34));
        assert_eq!(
            size_label_origin(IRect::new(50, 100, 10, 10), 2.0),
            (50, 48)
        );
    }

    #[test]
    fn hint_is_centred_near_the_top() {
        let monitor = IRect::new(-1920, 0, 1920, 1080);
        assert_eq!(
            hint_rect(monitor, 600, 30, 1.0),
            IRect::new(-1260, 24, 600, 30)
        );
    }

    #[test]
    fn dimming_matches_the_overlay_colour() {
        let src = [255, 100, 0, 0];
        let mut dst = [0u8; 4];
        dim_pixels(&src, &mut dst);
        assert_eq!(dst, [140, 55, 0, 255]);
    }

    #[test]
    fn size_text_uses_a_times_sign() {
        assert_eq!(size_text(IRect::new(0, 0, 640, 480)), "640 × 480");
    }
}
