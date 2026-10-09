//! A slightly tapered arrow with a broad, rounded head, white outline and soft shadow
//! (port of `ArrowAnnotation.cs`).

use tiny_skia::{FillRule, LineJoin, Paint, Path, PathBuilder, PixmapMut, Stroke, Transform};

use super::Rgb;
use crate::draw;
use crate::geometry::{Point, Rect, Vector};

#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub start: Point,
    pub end: Point,
    pub color: Rgb,
    pub thickness: f64,
}

impl Arrow {
    /// A new arrow starts as a point; dragging moves its end.
    pub fn new(start: Point, color: Rgb, thickness: f64) -> Self {
        Self {
            start,
            end: start,
            color,
            thickness,
        }
    }

    fn length(&self) -> f64 {
        (self.end - self.start).length()
    }

    /// Too short to keep when the drag ends.
    pub fn is_too_short(&self) -> bool {
        self.length() < self.thickness * 2.0
    }

    pub fn handles(&self) -> Vec<Point> {
        vec![self.start, self.end]
    }

    pub fn set_handles(&mut self, handles: &[Point]) {
        self.start = handles[0];
        self.end = handles[1];
    }

    pub fn hit_test(&self, p: Point, tolerance: f64) -> bool {
        let t = self.thickness;
        distance_to_segment(p, self.start, self.end) <= (t * 1.2).max(t / 2.0 + tolerance)
    }

    /// Covers the head's wings.
    pub fn bounds(&self) -> Rect {
        let t = self.thickness * 2.3;
        Rect::from_points(self.start, self.end).inflate(t, t)
    }

    pub fn render(&self, pixmap: &mut PixmapMut<'_>, transform: Transform) {
        if self.length() < 1.0 {
            return;
        }
        let Some(path) = self.path() else { return };
        let t = self.thickness;

        let mut paint = Paint::default();
        let offset = (t * 0.35).max(1.0) as f32;
        paint.set_color(draw::color(0, 0, 0, 70));
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            transform.pre_translate(offset, offset),
            None,
        );

        let Rgb(r, g, b) = self.color;
        paint.set_color(draw::color(r, g, b, 255));
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

        paint.set_color(draw::color(255, 255, 255, 255));
        let stroke = Stroke {
            width: (t * 0.3).max(1.5) as f32,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, transform, None);
    }

    /// The outline from the tail round to the tip and back, each corner with its own rounding.
    pub fn path(&self) -> Option<Path> {
        let vector = self.end - self.start;
        let length = vector.length();
        if length == 0.0 {
            return None;
        }
        let dir = vector * (1.0 / length);
        let normal = Vector::new(-dir.y, dir.x);

        let t = self.thickness;
        let head_length = (t * 3.2).min(length * 0.6);
        let head_half_width = (t * 2.3).max(head_length * 0.6);
        let shaft_half_width = t / 2.0;
        let tail_half_width = t * 0.45;
        let head_base = self.end - dir * head_length;

        let corners = [
            (self.start - normal * tail_half_width, tail_half_width),
            (head_base - normal * shaft_half_width, t * 0.3),
            (head_base - normal * head_half_width, t * 0.8),
            (self.end, t * 0.7),
            (head_base + normal * head_half_width, t * 0.8),
            (head_base + normal * shaft_half_width, t * 0.3),
            (self.start + normal * tail_half_width, tail_half_width),
        ];

        let mut pb = PathBuilder::new();
        let n = corners.len();
        for i in 0..n {
            let prev = corners[(i + n - 1) % n].0;
            let next = corners[(i + 1) % n].0;
            let (corner, radius) = corners[i];

            // Never round more than half of either adjacent edge.
            let to_prev = prev - corner;
            let to_next = next - corner;
            let r = radius.min(to_prev.length().min(to_next.length()) / 2.0);
            let entry = corner + to_prev.unit() * r;
            let exit = corner + to_next.unit() * r;

            if i == 0 {
                pb.move_to(entry.x as f32, entry.y as f32);
            } else {
                pb.line_to(entry.x as f32, entry.y as f32);
            }
            pb.quad_to(
                corner.x as f32,
                corner.y as f32,
                exit.x as f32,
                exit.y as f32,
            );
        }
        pb.close();
        pb.finish()
    }
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let length_squared = ab.length_squared();
    if length_squared == 0.0 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / length_squared).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrow(x1: f64, y1: f64, x2: f64, y2: f64) -> Arrow {
        Arrow {
            start: Point::new(x1, y1),
            end: Point::new(x2, y2),
            color: Rgb(255, 59, 48),
            thickness: 7.0,
        }
    }

    #[test]
    fn short_arrows_are_dropped() {
        assert!(arrow(10.0, 10.0, 20.0, 10.0).is_too_short());
        assert!(!arrow(10.0, 10.0, 30.0, 10.0).is_too_short());
    }

    #[test]
    fn hit_test_follows_the_shaft() {
        let a = arrow(10.0, 10.0, 110.0, 10.0);
        assert!(a.hit_test(Point::new(60.0, 14.0), 4.0));
        assert!(!a.hit_test(Point::new(60.0, 30.0), 4.0));
        assert!(!a.hit_test(Point::new(130.0, 10.0), 4.0));
    }

    #[test]
    fn renders_fill_outline_and_shadow() {
        let a = arrow(20.0, 50.0, 180.0, 50.0);
        let mut pixels = vec![0u8; 200 * 100 * 4];
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 200, 100).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(128, 128, 128, 255));
        a.render(&mut pixmap, Transform::identity());

        let at = |x: usize, y: usize| -> [u8; 4] {
            pixels[(y * 200 + x) * 4..][..4].try_into().unwrap()
        };
        assert_eq!(
            at(100, 50),
            [48, 59, 255, 255],
            "shaft is the arrow colour (BGRA)"
        );
        assert_eq!(at(5, 5), [128, 128, 128, 255], "far away is untouched");
        let edge = at(100, 46); // on the white outline of the 7 px shaft
        assert!(
            edge[0] > 200 && edge[1] > 200 && edge[2] > 200,
            "outline is white: {edge:?}"
        );
        let shadow = at(100, 55); // just below the outline: only the shadow (offset 2.45 px)
        assert!(
            shadow[0] < 128 && shadow[0] > 60,
            "shadow darkens: {shadow:?}"
        );
    }

    #[test]
    fn head_shrinks_for_short_arrows() {
        // A 20 px arrow with a 7 px thickness: the head can be at most 60% of the length.
        let path = arrow(0.0, 0.0, 20.0, 0.0).path().unwrap();
        let bounds = path.bounds();
        assert!(bounds.left() >= -1.0 && bounds.right() <= 20.5);
    }
}
