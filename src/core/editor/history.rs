//! Undo/redo over the annotation list: adding, removing and reshaping annotations
//! (port of `EditHistory.cs`), and changing the crop. Annotations are identified by their id.

use super::annotation::Annotation;
use crate::geometry::{IRect, Point};

#[derive(Debug, Clone)]
enum Edit {
    Added(Annotation),
    Removed {
        index: usize,
        annotation: Annotation,
    },
    Reshaped {
        id: u32,
        before: Vec<Point>,
        after: Vec<Point>,
    },
    Cropped {
        before: Option<IRect>,
        after: Option<IRect>,
    },
}

/// What the history changes: the annotations and the crop.
pub struct Document<'a> {
    pub annotations: &'a mut Vec<Annotation>,
    pub crop: &'a mut Option<IRect>,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn add(&mut self, list: &mut Vec<Annotation>, annotation: Annotation) {
        list.push(annotation.clone());
        self.push(Edit::Added(annotation));
    }

    pub fn remove(&mut self, list: &mut Vec<Annotation>, id: u32) {
        let Some(index) = list.iter().position(|a| a.id == id) else {
            return;
        };
        let annotation = list.remove(index);
        self.push(Edit::Removed { index, annotation });
    }

    /// Records a move or reshape that was already applied while dragging.
    pub fn record_reshape(&mut self, id: u32, before: Vec<Point>, after: Vec<Point>) {
        self.push(Edit::Reshaped { id, before, after });
    }

    /// Records a crop change that was already applied.
    pub fn record_crop(&mut self, before: Option<IRect>, after: Option<IRect>) {
        self.push(Edit::Cropped { before, after });
    }

    pub fn undo(&mut self, doc: Document<'_>) -> bool {
        let Some(edit) = self.undo.pop() else {
            return false;
        };
        let list = doc.annotations;
        match &edit {
            Edit::Added(annotation) => list.retain(|a| a.id != annotation.id),
            Edit::Removed { index, annotation } => {
                list.insert((*index).min(list.len()), annotation.clone())
            }
            Edit::Reshaped { id, before, .. } => set_handles(list, *id, before),
            Edit::Cropped { before, .. } => *doc.crop = *before,
        }
        self.redo.push(edit);
        true
    }

    pub fn redo(&mut self, doc: Document<'_>) -> bool {
        let Some(edit) = self.redo.pop() else {
            return false;
        };
        let list = doc.annotations;
        match &edit {
            Edit::Added(annotation) => list.push(annotation.clone()),
            Edit::Removed { annotation, .. } => list.retain(|a| a.id != annotation.id),
            Edit::Reshaped { id, after, .. } => set_handles(list, *id, after),
            Edit::Cropped { after, .. } => *doc.crop = *after,
        }
        self.undo.push(edit);
        true
    }

    fn push(&mut self, edit: Edit) {
        self.undo.push(edit);
        self.redo.clear();
    }
}

fn set_handles(list: &mut [Annotation], id: u32, handles: &[Point]) {
    if let Some(annotation) = list.iter_mut().find(|a| a.id == id) {
        annotation.set_handles(handles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Rgb;
    use crate::editor::annotation::Shape;
    use crate::editor::arrow::Arrow;

    fn doc<'a>(annotations: &'a mut Vec<Annotation>, crop: &'a mut Option<IRect>) -> Document<'a> {
        Document { annotations, crop }
    }

    fn arrow(id: u32, x: f64) -> Annotation {
        let mut a = Arrow::new(Point::new(x, 0.0), Rgb(0, 0, 0), 7.0);
        a.end = Point::new(x + 50.0, 0.0);
        Annotation {
            id,
            shape: Shape::Arrow(a),
        }
    }

    fn ids(list: &[Annotation]) -> Vec<u32> {
        list.iter().map(|a| a.id).collect()
    }

    #[test]
    fn add_undo_redo() {
        let mut list = Vec::new();
        let mut crop = None;
        let mut history = History::default();
        history.add(&mut list, arrow(1, 0.0));
        history.add(&mut list, arrow(2, 10.0));
        assert_eq!(ids(&list), [1, 2]);

        assert!(history.undo(doc(&mut list, &mut crop)));
        assert_eq!(ids(&list), [1]);
        assert!(history.can_redo());
        assert!(history.redo(doc(&mut list, &mut crop)));
        assert_eq!(ids(&list), [1, 2]);
        assert!(!history.redo(doc(&mut list, &mut crop)));
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut list = Vec::new();
        let mut crop = None;
        let mut history = History::default();
        history.add(&mut list, arrow(1, 0.0));
        history.undo(doc(&mut list, &mut crop));
        history.add(&mut list, arrow(2, 0.0));
        assert!(!history.can_redo());
        assert_eq!(ids(&list), [2]);
    }

    #[test]
    fn removal_restores_the_original_position() {
        let mut list = Vec::new();
        let mut crop = None;
        let mut history = History::default();
        for id in 1..=3 {
            history.add(&mut list, arrow(id, 0.0));
        }
        history.remove(&mut list, 2);
        assert_eq!(ids(&list), [1, 3]);
        history.undo(doc(&mut list, &mut crop));
        assert_eq!(ids(&list), [1, 2, 3]);
        history.redo(doc(&mut list, &mut crop));
        assert_eq!(ids(&list), [1, 3]);
    }

    #[test]
    fn reshape_undo_redo() {
        let mut list = Vec::new();
        let mut crop = None;
        let mut history = History::default();
        history.add(&mut list, arrow(1, 0.0));
        let before = list[0].handles().unwrap();
        let after = vec![Point::new(5.0, 5.0), Point::new(90.0, 40.0)];
        list[0].set_handles(&after);
        history.record_reshape(1, before.clone(), after.clone());

        history.undo(doc(&mut list, &mut crop));
        assert_eq!(list[0].handles().unwrap(), before);
        history.redo(doc(&mut list, &mut crop));
        assert_eq!(list[0].handles().unwrap(), after);
    }

    #[test]
    fn crop_undo_redo() {
        let mut list = Vec::new();
        let mut history = History::default();
        let first = IRect::new(10, 10, 50, 50);
        let second = IRect::new(20, 20, 30, 30);
        let mut crop = Some(first);
        history.record_crop(None, crop);
        crop = Some(second);
        history.record_crop(Some(first), crop);

        history.undo(doc(&mut list, &mut crop));
        assert_eq!(crop, Some(first));
        history.undo(doc(&mut list, &mut crop));
        assert_eq!(crop, None);
        history.redo(doc(&mut list, &mut crop));
        assert_eq!(crop, Some(first));
    }
}
