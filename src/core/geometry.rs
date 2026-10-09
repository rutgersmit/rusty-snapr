//! Small geometry types. Points and vectors are in image pixels, as `f64` like WPF's.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Vector {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    pub fn dot(self, other: Vector) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// The same direction with length 1; a zero vector stays zero.
    pub fn unit(self) -> Vector {
        let length = self.length();
        if length > 0.0 {
            self * (1.0 / length)
        } else {
            self
        }
    }
}

impl Sub for Point {
    type Output = Vector;
    fn sub(self, other: Point) -> Vector {
        Vector::new(self.x - other.x, self.y - other.y)
    }
}

impl Add<Vector> for Point {
    type Output = Point;
    fn add(self, v: Vector) -> Point {
        Point::new(self.x + v.x, self.y + v.y)
    }
}

impl Sub<Vector> for Point {
    type Output = Point;
    fn sub(self, v: Vector) -> Point {
        Point::new(self.x - v.x, self.y - v.y)
    }
}

impl Add for Vector {
    type Output = Vector;
    fn add(self, v: Vector) -> Vector {
        Vector::new(self.x + v.x, self.y + v.y)
    }
}

impl Sub for Vector {
    type Output = Vector;
    fn sub(self, v: Vector) -> Vector {
        Vector::new(self.x - v.x, self.y - v.y)
    }
}

impl Neg for Vector {
    type Output = Vector;
    fn neg(self) -> Vector {
        Vector::new(-self.x, -self.y)
    }
}

impl Mul<f64> for Vector {
    type Output = Vector;
    fn mul(self, factor: f64) -> Vector {
        Vector::new(self.x * factor, self.y * factor)
    }
}

/// A rectangle in floating-point coordinates. Width and height are never negative.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The rectangle spanned by two corners, in any order.
    pub fn from_points(a: Point, b: Point) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        Self::new(x, y, (a.x - b.x).abs(), (a.y - b.y).abs())
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Grows the rectangle by `dx`/`dy` on every side.
    pub fn inflate(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(
            self.x - dx,
            self.y - dy,
            (self.width + 2.0 * dx).max(0.0),
            (self.height + 2.0 * dy).max(0.0),
        )
    }

    pub fn offset(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// Scales every coordinate by `factor` and rounds to whole pixels, keeping at least 1×1.
    pub fn to_pixels(&self, factor: f64) -> IRect {
        let x = (self.x * factor).round() as i32;
        let y = (self.y * factor).round() as i32;
        let right = (self.right() * factor).round() as i32;
        let bottom = (self.bottom() * factor).round() as i32;
        IRect::new(x, y, (right - x).max(1), (bottom - y).max(1))
    }
}

/// A rectangle in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// Moves and shrinks the rectangle so it lies inside a `width`×`height` image, keeping at
    /// least one pixel. Used when rounding at the screen edges pushes a selection just outside.
    pub fn clamp_within(&self, width: i32, height: i32) -> IRect {
        let x = self.x.clamp(0, (width - 1).max(0));
        let y = self.y.clamp(0, (height - 1).max(0));
        let w = self.width.clamp(1, (width - x).max(1));
        let h = self.height.clamp(1, (height - y).max(1));
        IRect::new(x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_from_points_normalises_corners() {
        let r = Rect::from_points(Point::new(10.0, 5.0), Point::new(2.0, 9.0));
        assert_eq!(r, Rect::new(2.0, 5.0, 8.0, 4.0));
    }

    #[test]
    fn to_pixels_rounds_edges_and_keeps_one_pixel() {
        let r = Rect::new(1.2, 1.6, 0.1, 0.1);
        assert_eq!(r.to_pixels(1.0), IRect::new(1, 2, 1, 1));
        assert_eq!(
            Rect::new(10.0, 10.0, 20.0, 30.0).to_pixels(1.5),
            IRect::new(15, 15, 30, 45)
        );
    }

    #[test]
    fn clamp_within_keeps_rect_on_the_image() {
        assert_eq!(
            IRect::new(-3, -2, 10, 10).clamp_within(100, 50),
            IRect::new(0, 0, 10, 10)
        );
        assert_eq!(
            IRect::new(95, 45, 10, 10).clamp_within(100, 50),
            IRect::new(95, 45, 5, 5)
        );
        assert_eq!(
            IRect::new(120, 70, 10, 10).clamp_within(100, 50),
            IRect::new(99, 49, 1, 1)
        );
    }

    #[test]
    fn vector_unit_and_length() {
        assert_eq!(Vector::new(3.0, 4.0).length(), 5.0);
        assert_eq!(Vector::new(0.0, 2.0).unit(), Vector::new(0.0, 1.0));
        assert_eq!(Vector::default().unit(), Vector::default());
    }
}
