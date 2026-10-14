//! A highlighter stroke: a broad, translucent band that colours what is under it like a marker
//! does (multiplied, so dark text stays dark).
//!
//! People mostly highlight a line of text, dragging roughly from left to right with a shaky hand.
//! The band doesn't trace every wobble of the mouse: a stroke that is more or less straight
//! becomes one straight band along the best-fitting line, and snaps to exactly horizontal (or
//! vertical) when it is close. Only a clearly curved stroke is followed, and then smoothed.

use tiny_skia::{BlendMode, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

use super::Rgb;
use crate::draw;
use crate::geometry::{Point, Rect, Vector};

/// How strongly the colour shows (multiplied onto the image).
const OPACITY: u8 = 150;
/// Strokes within this angle of horizontal or vertical are snapped to it.
const SNAP_DEGREES: f64 = 7.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    /// The band's centre line: two points when straight, more when it follows a curve.
    pub points: Vec<Point>,
    pub color: Rgb,
    /// Height of the band in image pixels.
    pub thickness: f64,
    /// The mouse positions of the creating drag; empty once the stroke is finished.
    raw: Vec<Point>,
}

impl Highlight {
    /// A new stroke starts as a point; dragging extends it with [`Self::extend`].
    pub fn new(start: Point, color: Rgb, thickness: f64) -> Self {
        Self {
            points: vec![start],
            color,
            thickness,
            raw: vec![start],
        }
    }

    /// Adds a mouse position to the stroke being drawn and reshapes it.
    pub fn extend(&mut self, p: Point) {
        if self.raw.last() == Some(&p) {
            return;
        }
        self.raw.push(p);
        self.points = shape(&self.raw, self.thickness);
    }

    /// Ends the creating drag; the shape is fixed from now on.
    pub fn finish(&mut self) {
        self.raw = Vec::new();
    }

    fn length(&self) -> f64 {
        self.points.windows(2).map(|w| (w[1] - w[0]).length()).sum()
    }

    /// Too short to keep when the drag ends.
    pub fn is_too_short(&self) -> bool {
        self.length() < self.thickness * 0.5
    }

    pub fn handles(&self) -> Vec<Point> {
        self.points.clone()
    }

    pub fn set_handles(&mut self, handles: &[Point]) {
        self.points = handles.to_vec();
    }

    /// A straight band can be stretched by its ends, like an arrow; a curved one only moved.
    pub fn has_end_handles(&self) -> bool {
        self.points.len() == 2
    }

    pub fn hit_test(&self, p: Point, tolerance: f64) -> bool {
        let reach = self.thickness / 2.0 + tolerance;
        match self.points.as_slice() {
            [single] => (p - *single).length() <= reach,
            points => points
                .windows(2)
                .any(|w| distance_to_segment(p, w[0], w[1]) <= reach),
        }
    }

    pub fn bounds(&self) -> Rect {
        let (mut min, mut max) = (self.points[0], self.points[0]);
        for p in &self.points {
            min = Point::new(min.x.min(p.x), min.y.min(p.y));
            max = Point::new(max.x.max(p.x), max.y.max(p.y));
        }
        let half = self.thickness / 2.0;
        Rect::from_points(min, max).inflate(half, half)
    }

    pub fn render(&self, pixmap: &mut PixmapMut<'_>, transform: Transform) {
        if self.points.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(self.points[0].x as f32, self.points[0].y as f32);
        for p in &self.points[1..] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        let Some(path) = pb.finish() else { return };

        // One stroke for the whole band, so where it crosses itself it doesn't get darker.
        let Rgb(r, g, b) = self.color;
        let mut paint = Paint::default();
        paint.set_color(draw::color(r, g, b, OPACITY));
        paint.blend_mode = BlendMode::Multiply;
        let stroke = Stroke {
            width: self.thickness as f32,
            // Flat ends like a chisel tip; rounded where a curved stroke bends.
            line_cap: LineCap::Butt,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, transform, None);
    }
}

/// The band's centre line for a stroke drawn through `raw`.
fn shape(raw: &[Point], thickness: f64) -> Vec<Point> {
    let points = resample(raw, (thickness / 4.0).max(1.0));
    if points.len() < 2 {
        return points;
    }

    let n = points.len() as f64;
    let center = Point::new(
        points.iter().map(|p| p.x).sum::<f64>() / n,
        points.iter().map(|p| p.y).sum::<f64>() / n,
    );
    let direction = fitted_direction(&points, center);

    // How far the stroke strays from that line. A long stroke may sag a little more: the arm
    // pivots at the elbow, so long "straight" lines come out as slight arcs.
    let length: f64 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let tolerance = thickness * 0.6 + length * 0.03;
    let along = |p: &Point| (*p - center).dot(direction);
    let across = |p: &Point| {
        let v = *p - center;
        (v.x * direction.y - v.y * direction.x).abs()
    };
    if points.iter().all(|p| across(p) <= tolerance) {
        let from = points.iter().map(along).fold(f64::INFINITY, f64::min);
        let to = points.iter().map(along).fold(f64::NEG_INFINITY, f64::max);
        return vec![center + direction * from, center + direction * to];
    }

    smooth(
        &points,
        ((thickness * 1.5) / (thickness / 4.0).max(1.0)).round() as usize,
    )
}

/// The direction of the line that best fits the points (least squares, perpendicular distance),
/// pointing the way the stroke was drawn and snapped to horizontal or vertical when close.
fn fitted_direction(points: &[Point], center: Point) -> Vector {
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in points {
        let v = *p - center;
        sxx += v.x * v.x;
        syy += v.y * v.y;
        sxy += v.x * v.y;
    }
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let mut direction = Vector::new(angle.cos(), angle.sin());

    let snap = SNAP_DEGREES.to_radians().sin();
    if direction.y.abs() <= snap {
        direction = Vector::new(1.0, 0.0);
    } else if direction.x.abs() <= snap {
        direction = Vector::new(0.0, 1.0);
    }

    let drawn = points[points.len() - 1] - points[0];
    if drawn.dot(direction) < 0.0 {
        -direction
    } else {
        direction
    }
}

/// Points along the polyline `raw`, about `step` apart, keeping both ends.
fn resample(raw: &[Point], step: f64) -> Vec<Point> {
    let mut out = vec![raw[0]];
    let mut last = raw[0];
    for &p in &raw[1..] {
        let mut v = p - last;
        while v.length() >= step {
            last = last + v.unit() * step;
            out.push(last);
            v = p - last;
        }
    }
    let end = raw[raw.len() - 1];
    if (end - out[out.len() - 1]).length() > 0.0 {
        out.push(end);
    }
    out
}

/// A moving average over up to `radius` neighbours on each side; the window narrows towards the
/// ends so the stroke keeps its start and end points.
fn smooth(points: &[Point], radius: usize) -> Vec<Point> {
    let last = points.len() - 1;
    (0..points.len())
        .map(|i| {
            let r = radius.min(i).min(last - i);
            let window = &points[i - r..=i + r];
            let n = window.len() as f64;
            Point::new(
                window.iter().map(|p| p.x).sum::<f64>() / n,
                window.iter().map(|p| p.y).sum::<f64>() / n,
            )
        })
        .collect()
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

    const YELLOW: Rgb = Rgb(0xFF, 0xD6, 0x0A);

    /// A stroke through the given points, as the mouse would report them.
    fn stroke(points: &[(f64, f64)]) -> Highlight {
        let mut h = Highlight::new(Point::new(points[0].0, points[0].1), YELLOW, 18.0);
        for &(x, y) in &points[1..] {
            h.extend(Point::new(x, y));
        }
        h.finish();
        h
    }

    /// A shaky left-to-right drag along y = 50.
    fn shaky_line(slope: f64) -> Vec<(f64, f64)> {
        (0..=60)
            .map(|i| {
                let x = 20.0 + i as f64 * 5.0;
                let wobble = [0.0, 3.0, -2.0, 4.0, -3.0, 1.0, -4.0][i % 7];
                (x, 50.0 + wobble + (x - 20.0) * slope)
            })
            .collect()
    }

    #[test]
    fn a_shaky_horizontal_drag_becomes_a_straight_horizontal_band() {
        let h = stroke(&shaky_line(0.0));
        assert_eq!(h.points.len(), 2);
        let (a, b) = (h.points[0], h.points[1]);
        assert_eq!(a.y, b.y, "exactly horizontal");
        assert!((a.y - 50.0).abs() < 1.5, "through the middle: {a:?}");
        assert!((a.x - 20.0).abs() < 0.01 && (b.x - 320.0).abs() < 0.01);
    }

    #[test]
    fn a_slightly_sloping_drag_is_snapped_to_horizontal() {
        // About 4°, as when the hand drifts while dragging along a line of text.
        let h = stroke(&shaky_line(0.07));
        assert_eq!(h.points.len(), 2);
        assert_eq!(h.points[0].y, h.points[1].y);
    }

    #[test]
    fn a_clearly_sloping_drag_keeps_its_angle_but_not_its_wobble() {
        let h = stroke(&shaky_line(0.5));
        assert_eq!(h.points.len(), 2);
        let v = h.points[1] - h.points[0];
        assert!((v.y / v.x - 0.5).abs() < 0.02, "{v:?}");
    }

    #[test]
    fn a_gentle_arc_over_a_long_line_still_counts_as_straight() {
        let points: Vec<(f64, f64)> = (0..=100)
            .map(|i| {
                let t = i as f64 / 100.0;
                (20.0 + 600.0 * t, 100.0 - 16.0 * 4.0 * t * (1.0 - t))
            })
            .collect();
        let h = stroke(&points);
        assert_eq!(h.points.len(), 2);
    }

    #[test]
    fn a_right_to_left_drag_keeps_its_direction() {
        let mut points = shaky_line(0.0);
        points.reverse();
        let h = stroke(&points);
        assert!(h.points[0].x > h.points[1].x);
    }

    #[test]
    fn a_vertical_drag_snaps_to_vertical() {
        let points: Vec<(f64, f64)> = shaky_line(0.0).iter().map(|&(x, y)| (y, x)).collect();
        let h = stroke(&points);
        assert_eq!(h.points.len(), 2);
        assert_eq!(h.points[0].x, h.points[1].x);
    }

    #[test]
    fn a_curve_is_followed_smoothly() {
        // A quarter circle with a jittery hand.
        let points: Vec<(f64, f64)> = (0..=90)
            .map(|deg| {
                let a = (deg as f64).to_radians();
                let jitter = if deg % 2 == 0 { 2.0 } else { -2.0 };
                let r = 150.0 + jitter;
                (20.0 + r * a.cos(), 20.0 + r * a.sin())
            })
            .collect();
        let h = stroke(&points);
        assert!(h.points.len() > 2, "not straightened");
        // Smoothed: the band stays near the circle without the jitter.
        for p in &h.points[3..h.points.len() - 3] {
            let r = (*p - Point::new(20.0, 20.0)).length();
            assert!((r - 150.0).abs() < 2.0, "r = {r}");
        }
        assert!(!h.has_end_handles());
    }

    #[test]
    fn a_click_is_too_short() {
        assert!(stroke(&[(10.0, 10.0)]).is_too_short());
        assert!(stroke(&[(10.0, 10.0), (13.0, 11.0)]).is_too_short());
        assert!(!stroke(&[(10.0, 10.0), (60.0, 10.0)]).is_too_short());
    }

    #[test]
    fn hit_test_covers_the_band() {
        let h = stroke(&[(20.0, 50.0), (120.0, 50.0)]);
        assert!(h.hit_test(Point::new(70.0, 58.0), 4.0));
        assert!(!h.hit_test(Point::new(70.0, 75.0), 4.0));
        assert!(!h.hit_test(Point::new(140.0, 50.0), 4.0));
    }

    #[test]
    fn colours_like_a_marker_and_keeps_text_dark() {
        let h = stroke(&[(20.0, 50.0), (180.0, 50.0)]);
        let mut pixels = vec![255u8; 200 * 100 * 4];
        // A black "letter" in the band.
        for y in 45..55 {
            pixels[(y * 200 + 100) * 4..][..3].copy_from_slice(&[0, 0, 0]);
        }
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 200, 100).unwrap();
        h.render(&mut pixmap, Transform::identity());

        let at = |x: usize, y: usize| -> [u8; 4] {
            pixels[(y * 200 + x) * 4..][..4].try_into().unwrap()
        };
        let paper = at(60, 50);
        assert_eq!(paper[2], 255, "red stays (BGRA): {paper:?}");
        assert!(
            paper[0] < 120,
            "blue is taken out, so it looks yellow: {paper:?}"
        );
        assert_eq!(at(100, 50), [0, 0, 0, 255], "text stays black");
        assert_eq!(
            at(60, 70),
            [255, 255, 255, 255],
            "outside the band is untouched"
        );
        assert_eq!(at(10, 50), [255, 255, 255, 255], "flat ends");
    }
}
