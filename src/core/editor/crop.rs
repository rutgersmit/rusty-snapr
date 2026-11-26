//! Cropping: the part of the screenshot that is kept. The crop is not applied to the screenshot
//! itself but kept as a rectangle next to it, so it can be changed or undone at any time and
//! annotations keep their coordinates. Copying and saving cut the flattened image to it.
//!
//! This module holds the geometry (drawing a new rectangle, moving it, dragging its edges) and
//! the frame the editor shows while the crop tool is active.

use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Rect as SkRect, Stroke, Transform};

use super::chrome::ACCENT;
use crate::draw;
use crate::geometry::{IRect, Point, Vector};

/// The smallest crop, in image pixels; anything smaller is treated as a click.
pub const MIN_SIZE: i32 = 4;

/// Which sides of the crop a drag moves. A corner moves two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Edges {
    /// The resize cursor for these edges.
    pub fn cursor(self) -> ResizeCursor {
        match (self.left || self.right, self.top || self.bottom) {
            (true, true) if self.left == self.top => ResizeCursor::NwSe,
            (true, true) => ResizeCursor::NeSw,
            (true, false) => ResizeCursor::WE,
            _ => ResizeCursor::NS,
        }
    }
}

/// The direction of the double arrow shown over an edge or corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeCursor {
    /// Top-left or bottom-right corner.
    NwSe,
    /// Top-right or bottom-left corner.
    NeSw,
    /// Left or right edge.
    WE,
    /// Top or bottom edge.
    NS,
}

/// The rectangle spanned by two points, rounded to whole pixels and kept on a
/// `width`×`height` image.
pub fn from_points(a: Point, b: Point, width: u32, height: u32) -> IRect {
    let clamp = |v: f64, max: u32| v.round().clamp(0.0, max as f64) as i32;
    let (x1, x2) = (clamp(a.x.min(b.x), width), clamp(a.x.max(b.x), width));
    let (y1, y2) = (clamp(a.y.min(b.y), height), clamp(a.y.max(b.y), height));
    IRect::new(x1, y1, x2 - x1, y2 - y1)
}

/// Whether a crop is large enough to keep.
pub fn is_usable(rect: IRect) -> bool {
    rect.width >= MIN_SIZE && rect.height >= MIN_SIZE
}

/// The edges of `rect` within `reach` of `p` (corners first), if any.
pub fn edges_at(rect: IRect, p: Point, reach: f64) -> Option<Edges> {
    let (left, top) = (rect.x as f64, rect.y as f64);
    let (right, bottom) = (rect.right() as f64, rect.bottom() as f64);
    let inside_x = p.x >= left - reach && p.x <= right + reach;
    let inside_y = p.y >= top - reach && p.y <= bottom + reach;
    if !inside_x || !inside_y {
        return None;
    }
    // Outside the crop an edge reaches the full distance; inside at most a quarter of the
    // crop, so a small or thin crop keeps a middle that moves it instead of resizing.
    let near = |v: f64, lo: f64, hi: f64| {
        let inner = reach.min((hi - lo) / 4.0);
        let near_lo = v >= lo - reach && v <= lo + inner;
        let near_hi = v <= hi + reach && v >= hi - inner;
        let lo_is_nearer = (v - lo).abs() <= (hi - v).abs();
        (
            near_lo && (!near_hi || lo_is_nearer),
            near_hi && (!near_lo || !lo_is_nearer),
        )
    };
    let (l, r) = near(p.x, left, right);
    let (t, b) = near(p.y, top, bottom);
    let edges = Edges {
        left: l,
        top: t,
        right: r,
        bottom: b,
    };
    (edges != Edges::default()).then_some(edges)
}

/// `original` with the given edges moved by `delta`, kept on the image and at least
/// [`MIN_SIZE`] wide and high (an edge stops instead of crossing the opposite one).
pub fn resize(original: IRect, edges: Edges, delta: Vector, width: u32, height: u32) -> IRect {
    let (w, h) = (width as i32, height as i32);
    let (dx, dy) = (delta.x.round() as i32, delta.y.round() as i32);
    let min_w = MIN_SIZE.min(w);
    let min_h = MIN_SIZE.min(h);
    let (mut left, mut top) = (original.x, original.y);
    let (mut right, mut bottom) = (original.right(), original.bottom());
    if edges.left {
        left = (left + dx).clamp(0, (right - min_w).max(0));
    }
    if edges.right {
        right = (right + dx).clamp((left + min_w).min(w), w);
    }
    if edges.top {
        top = (top + dy).clamp(0, (bottom - min_h).max(0));
    }
    if edges.bottom {
        bottom = (bottom + dy).clamp((top + min_h).min(h), h);
    }
    IRect::new(left, top, right - left, bottom - top)
}

/// `original` moved by `delta`, kept on the image without changing its size.
pub fn moved(original: IRect, delta: Vector, width: u32, height: u32) -> IRect {
    let (dx, dy) = (delta.x.round() as i32, delta.y.round() as i32);
    let x = (original.x + dx).clamp(0, (width as i32 - original.width).max(0));
    let y = (original.y + dy).clamp(0, (height as i32 - original.height).max(0));
    IRect::new(x, y, original.width, original.height)
}

/// Whether `p` lies inside `rect`.
pub fn contains(rect: IRect, p: Point) -> bool {
    p.x >= rect.x as f64
        && p.x <= rect.right() as f64
        && p.y >= rect.y as f64
        && p.y <= rect.bottom() as f64
}

/// Draws the crop frame over the whole image: everything outside `crop` dimmed, a white outline
/// and handles at the corners and edge centres. `image` is the image size; `handle` the handle
/// size in image pixels; `transform` maps image pixels to `pixmap`.
pub fn render_frame(
    pixmap: &mut PixmapMut<'_>,
    crop: IRect,
    image: (u32, u32),
    handle: f64,
    transform: Transform,
) {
    let (w, h) = (image.0 as f32, image.1 as f32);
    let (x, y) = (crop.x as f32, crop.y as f32);
    let (r, b) = (crop.right() as f32, crop.bottom() as f32);

    // The parts that are cut off.
    let mut shade = Paint::default();
    shade.set_color(draw::color(0, 0, 0, 150));
    let mut pb = PathBuilder::new();
    for rect in [
        SkRect::from_ltrb(0.0, 0.0, w, y),
        SkRect::from_ltrb(0.0, b, w, h),
        SkRect::from_ltrb(0.0, y, x, b),
        SkRect::from_ltrb(r, y, w, b),
    ]
    .into_iter()
    .flatten()
    {
        pb.push_rect(rect);
    }
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &shade, FillRule::Winding, transform, None);
    }

    let Some(outline) = SkRect::from_ltrb(x, y, r, b) else {
        return;
    };
    let mut white = Paint::default();
    white.set_color(draw::color(255, 255, 255, 255));
    let line = Stroke {
        width: (handle * 0.25) as f32,
        ..Stroke::default()
    };
    let outline = PathBuilder::from_rect(outline);
    pixmap.stroke_path(&outline, &white, &line, transform, None);

    // Square handles, white with an accent edge, like the selection's.
    let mut accent = Paint::default();
    accent.set_color(draw::color(ACCENT.0, ACCENT.1, ACCENT.2, 255));
    let ring = Stroke {
        width: (handle * 0.3) as f32,
        ..Stroke::default()
    };
    let half = (handle / 2.0) as f32;
    let (cx, cy) = ((x + r) / 2.0, (y + b) / 2.0);
    for (hx, hy) in [
        (x, y),
        (cx, y),
        (r, y),
        (r, cy),
        (r, b),
        (cx, b),
        (x, b),
        (x, cy),
    ] {
        if let Some(square) = SkRect::from_xywh(hx - half, hy - half, 2.0 * half, 2.0 * half) {
            let path = PathBuilder::from_rect(square);
            pixmap.fill_path(&path, &white, FillRule::Winding, transform, None);
            pixmap.stroke_path(&path, &accent, &ring, transform, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: IRect = IRect::new(20, 20, 100, 60);

    #[test]
    fn from_points_in_any_order_stays_on_the_image() {
        assert_eq!(
            from_points(Point::new(80.4, 10.0), Point::new(-5.0, 300.0), 200, 100),
            IRect::new(0, 10, 80, 90)
        );
    }

    #[test]
    fn finds_corners_and_edges() {
        let corner = edges_at(RECT, Point::new(21.0, 18.0), 6.0).unwrap();
        assert_eq!(
            corner,
            Edges {
                left: true,
                top: true,
                ..Edges::default()
            }
        );
        assert_eq!(corner.cursor(), ResizeCursor::NwSe);
        let right = edges_at(RECT, Point::new(123.0, 50.0), 6.0).unwrap();
        assert_eq!(right.cursor(), ResizeCursor::WE);
        let bottom_left = edges_at(RECT, Point::new(18.0, 82.0), 6.0).unwrap();
        assert_eq!(bottom_left.cursor(), ResizeCursor::NeSw);
        assert_eq!(edges_at(RECT, Point::new(70.0, 50.0), 6.0), None, "inside");
        assert_eq!(edges_at(RECT, Point::new(70.0, 5.0), 6.0), None, "outside");
    }

    #[test]
    fn a_tiny_crop_still_has_both_sides() {
        let tiny = IRect::new(50, 50, 4, 4);
        let right = edges_at(tiny, Point::new(54.5, 52.0), 6.0).unwrap();
        assert!(right.right && !right.left);
    }

    #[test]
    fn a_small_crop_can_still_be_moved_from_its_middle() {
        let small = IRect::new(50, 50, 10, 10);
        assert_eq!(edges_at(small, Point::new(55.0, 55.0), 9.0), None);
        let thin = IRect::new(50, 20, 6, 60);
        assert_eq!(edges_at(thin, Point::new(53.0, 50.0), 9.0), None);
        // Just outside, the edges still reach the full distance.
        assert!(edges_at(small, Point::new(66.0, 55.0), 9.0).unwrap().right);
        assert!(edges_at(small, Point::new(52.0, 55.0), 9.0).unwrap().left);
    }

    #[test]
    fn resizing_keeps_a_minimum_size_and_stays_on_the_image() {
        let right = Edges {
            right: true,
            ..Edges::default()
        };
        assert_eq!(
            resize(RECT, right, Vector::new(500.0, 0.0), 200, 100),
            IRect::new(20, 20, 180, 60)
        );
        assert_eq!(
            resize(RECT, right, Vector::new(-500.0, 0.0), 200, 100),
            IRect::new(20, 20, MIN_SIZE, 60)
        );
        let top_left = Edges {
            left: true,
            top: true,
            ..Edges::default()
        };
        assert_eq!(
            resize(RECT, top_left, Vector::new(-50.0, 10.0), 200, 100),
            IRect::new(0, 30, 120, 50)
        );
    }

    #[test]
    fn moving_keeps_the_size() {
        assert_eq!(
            moved(RECT, Vector::new(500.0, -500.0), 200, 100),
            IRect::new(100, 0, 100, 60)
        );
    }

    #[test]
    fn the_frame_dims_only_what_is_cut_off() {
        let mut pixels = vec![255u8; 200 * 100 * 4];
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 200, 100).unwrap();
        render_frame(&mut pixmap, RECT, (200, 100), 5.0, Transform::identity());
        let at = |x: usize, y: usize| pixels[(y * 200 + x) * 4];
        assert!(at(5, 5) < 128, "outside is dimmed");
        assert_eq!(at(70, 50), 255, "inside is untouched");
    }
}
