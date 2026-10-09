//! Something drawn on top of the screenshot. Coordinates are in image pixels, and the same
//! render call is used for the on-screen preview and the exported image.

use tiny_skia::{PixmapMut, Transform};

use super::arrow::Arrow;
use crate::geometry::{Point, Rect};

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Arrow(Arrow),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Stable identity for undo/redo and selection.
    pub id: u32,
    pub shape: Shape,
}

impl Annotation {
    pub fn render(&self, pixmap: &mut PixmapMut<'_>, transform: Transform) {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.render(pixmap, transform),
        }
    }

    /// Too small to keep after the creating drag.
    pub fn is_empty(&self) -> bool {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.is_too_short(),
        }
    }

    /// The points that fully describe an editable annotation's shape (both ends of an arrow);
    /// `None` for annotations that can't be selected.
    pub fn handles(&self) -> Option<Vec<Point>> {
        match &self.shape {
            Shape::Arrow(arrow) => Some(arrow.handles()),
        }
    }

    pub fn set_handles(&mut self, handles: &[Point]) {
        match &mut self.shape {
            Shape::Arrow(arrow) => arrow.set_handles(handles),
        }
    }

    /// Whether `p` touches the drawn shape, with some slack for easier clicking.
    pub fn hit_test(&self, p: Point, tolerance: f64) -> bool {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.hit_test(p, tolerance),
        }
    }

    /// The area the selection outline is drawn around.
    pub fn bounds(&self) -> Rect {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.bounds(),
        }
    }
}
