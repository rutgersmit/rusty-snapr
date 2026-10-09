//! Helpers for drawing with tiny-skia on BGRA pixel buffers.
//!
//! Snapr's pixel buffers are BGRA (the Windows order) while tiny-skia thinks in RGBA. Blending
//! treats every channel the same way, so it is enough to swap red and blue when choosing a
//! colour: [`color`] does that.

use tiny_skia::{Color, Path, PathBuilder};

/// A colour for drawing on a BGRA buffer, given as normal RGBA.
pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(b, g, r, a)
}

/// A rectangle with circular corners of radius `r` (clamped to half the shorter side).
pub fn rounded_rect(x: f32, y: f32, width: f32, height: f32, r: f32) -> Option<Path> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let r = r.clamp(0.0, width.min(height) / 2.0);
    // Control point distance for a quarter circle drawn as one cubic Bézier.
    let k = r * 0.552_284_8;
    let (right, bottom) = (x + width, y + height);

    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(right - r, y);
    pb.cubic_to(right - r + k, y, right, y + r - k, right, y + r);
    pb.line_to(right, bottom - r);
    pb.cubic_to(
        right,
        bottom - r + k,
        right - r + k,
        bottom,
        right - r,
        bottom,
    );
    pb.line_to(x + r, bottom);
    pb.cubic_to(x + r - k, bottom, x, bottom - r + k, x, bottom - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{FillRule, Paint, PixmapMut, Transform};

    #[test]
    fn color_swaps_red_and_blue() {
        let mut pixels = vec![0u8; 4];
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 1, 1).unwrap();
        pixmap.fill(color(255, 128, 0, 255));
        assert_eq!(pixels, [0, 128, 255, 255]); // B, G, R, A
    }

    #[test]
    fn rounded_rect_covers_the_middle_but_not_the_corners() {
        let mut pixels = vec![0u8; 20 * 20 * 4];
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 20, 20).unwrap();
        let mut paint = Paint::default();
        paint.set_color(color(255, 255, 255, 255));
        let path = rounded_rect(0.0, 0.0, 20.0, 20.0, 8.0).unwrap();
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let at = |x: usize, y: usize| pixels[(y * 20 + x) * 4];
        assert_eq!(at(10, 10), 255);
        assert_eq!(at(10, 0), 255);
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(19, 19), 0);
    }

    #[test]
    fn empty_rect_has_no_path() {
        assert!(rounded_rect(0.0, 0.0, 0.0, 5.0, 2.0).is_none());
    }
}
