//! Something drawn on top of the screenshot. Coordinates are in image pixels, and the same
//! render call is used for the on-screen preview and the exported image.

use tiny_skia::{PixmapMut, Transform};

use super::arrow::Arrow;
use super::highlight::Highlight;
use super::number::Number;
use super::pixelate::Pixelate;
use crate::geometry::{Point, Rect};

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Arrow(Arrow),
    Highlight(Highlight),
    Number(Number),
    Pixelate(Pixelate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Stable identity for undo/redo and selection.
    pub id: u32,
    pub shape: Shape,
}

impl Annotation {
    /// Draws the annotation. `transform` may scale uniformly and translate (not rotate): the
    /// editor draws at screen size, exports at image size.
    pub fn render(&self, pixmap: &mut PixmapMut<'_>, transform: Transform) {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.render(pixmap, transform),
            Shape::Highlight(highlight) => highlight.render(pixmap, transform),
            Shape::Number(number) => number.render(pixmap, transform),
            Shape::Pixelate(pixelate) => pixelate.render(pixmap, transform),
        }
    }

    /// Too small to keep after the creating drag.
    pub fn is_empty(&self) -> bool {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.is_too_short(),
            Shape::Highlight(highlight) => highlight.is_too_short(),
            Shape::Number(_) => false,
            Shape::Pixelate(pixelate) => pixelate.is_empty(),
        }
    }

    /// Arrows, highlights and numbers can be selected and moved; pixelated regions can't.
    pub fn is_editable(&self) -> bool {
        !matches!(self.shape, Shape::Pixelate(_))
    }

    /// Whether the handles are end points that can be dragged one by one (an arrow, a straight
    /// highlight); otherwise the annotation can only be moved as a whole.
    pub fn has_end_handles(&self) -> bool {
        match &self.shape {
            Shape::Arrow(_) => true,
            Shape::Highlight(highlight) => highlight.has_end_handles(),
            Shape::Number(_) | Shape::Pixelate(_) => false,
        }
    }

    /// The points that fully describe an editable annotation's shape (both ends of an arrow, the
    /// centre line of a highlight, the centre of a number); `None` for annotations that can't be selected.
    pub fn handles(&self) -> Option<Vec<Point>> {
        match &self.shape {
            Shape::Arrow(arrow) => Some(arrow.handles()),
            Shape::Highlight(highlight) => Some(highlight.handles()),
            Shape::Number(number) => Some(number.handles()),
            Shape::Pixelate(_) => None,
        }
    }

    pub fn set_handles(&mut self, handles: &[Point]) {
        match &mut self.shape {
            Shape::Arrow(arrow) => arrow.set_handles(handles),
            Shape::Highlight(highlight) => highlight.set_handles(handles),
            Shape::Number(number) => number.set_handles(handles),
            Shape::Pixelate(_) => {}
        }
    }

    /// Whether `p` touches the drawn shape, with some slack for easier clicking.
    pub fn hit_test(&self, p: Point, tolerance: f64) -> bool {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.hit_test(p, tolerance),
            Shape::Highlight(highlight) => highlight.hit_test(p, tolerance),
            Shape::Number(number) => number.hit_test(p, tolerance),
            Shape::Pixelate(_) => false,
        }
    }

    /// The area the selection outline is drawn around.
    pub fn bounds(&self) -> Rect {
        match &self.shape {
            Shape::Arrow(arrow) => arrow.bounds(),
            Shape::Highlight(highlight) => highlight.bounds(),
            Shape::Number(number) => number.bounds(),
            Shape::Pixelate(pixelate) => {
                let r = pixelate.region();
                Rect::new(r.x as f64, r.y as f64, r.width as f64, r.height as f64)
            }
        }
    }
}
