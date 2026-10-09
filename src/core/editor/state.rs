//! The editor's state machine (port of the logic in `EditorWindow.xaml.cs`), without any window:
//! the window feeds it mouse positions in image pixels and commands, and repaints what it says
//! changed.

use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Stroke, StrokeDash, Transform};

use super::annotation::{Annotation, Shape};
use super::arrow::Arrow;
use super::chrome::ACCENT;
use super::history::History;
use super::number::Number;
use super::pixelate::Pixelate;
use super::render;
use super::{Bitmap, PALETTE, Tool, round_like_dotnet};
use crate::draw;
use crate::geometry::{Point, Vector};
use crate::hotkey::vk;

/// What needs repainting after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    /// The image area (annotations, the one being drawn, the selection).
    pub image: bool,
    /// The toolbar (tool, colour, undo/redo availability).
    pub toolbar: bool,
}

impl Changes {
    pub const NONE: Changes = Changes {
        image: false,
        toolbar: false,
    };
    pub const IMAGE: Changes = Changes {
        image: true,
        toolbar: false,
    };
    pub const TOOLBAR: Changes = Changes {
        image: false,
        toolbar: true,
    };
    pub const ALL: Changes = Changes {
        image: true,
        toolbar: true,
    };

    pub fn or(self, other: Changes) -> Changes {
        Changes {
            image: self.image || other.image,
            toolbar: self.toolbar || other.toolbar,
        }
    }
}

/// Something the user asked for with the keyboard or the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Deselects, or closes the editor when nothing is selected.
    Escape,
    /// Deletes the selected annotation.
    Delete,
    Undo,
    Redo,
    Copy,
    Save,
    Tool(Tool),
    Color(usize),
}

/// The editor's keyboard shortcuts.
pub fn command_for_key(key: u16, ctrl: bool, shift: bool) -> Option<Command> {
    const BACK: u16 = 0x08;
    const DELETE: u16 = 0x2E;
    const KEY_A: u16 = b'A' as u16;
    const KEY_B: u16 = b'B' as u16;
    const KEY_C: u16 = b'C' as u16;
    const KEY_N: u16 = b'N' as u16;
    const KEY_P: u16 = b'P' as u16;
    const KEY_S: u16 = b'S' as u16;
    const KEY_Y: u16 = b'Y' as u16;
    const KEY_Z: u16 = b'Z' as u16;
    const KEY_1: u16 = b'1' as u16;
    const KEY_5: u16 = b'5' as u16;
    Some(match key {
        vk::ESCAPE => Command::Escape,
        DELETE | BACK => Command::Delete,
        KEY_Z if ctrl && shift => Command::Redo,
        KEY_Z if ctrl => Command::Undo,
        KEY_Y if ctrl => Command::Redo,
        KEY_C if ctrl => Command::Copy,
        KEY_S if ctrl => Command::Save,
        KEY_A if !ctrl => Command::Tool(Tool::Arrow),
        KEY_P | KEY_B if !ctrl => Command::Tool(Tool::Pixelate),
        KEY_N if !ctrl => Command::Tool(Tool::Number),
        KEY_1..=KEY_5 if !ctrl => Command::Color((key - KEY_1) as usize),
        _ => return None,
    })
}

/// The mouse cursor over the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// Drawing.
    Cross,
    /// Over an arrow or number: it can be moved.
    Move,
    /// Over an end point of the selected arrow.
    Hand,
}

#[derive(Debug, Clone, PartialEq)]
enum Drag {
    None,
    Create,
    Move { start: Point, original: Vec<Point> },
    Handle { index: usize, original: Vec<Point> },
}

pub struct EditorState {
    image: Bitmap,
    annotations: Vec<Annotation>,
    history: History,
    next_id: u32,
    tool: Tool,
    color: usize,
    /// Display scale (1.0 = 96 DPI); sizes annotations and handles like the C# version.
    scale: f64,
    /// The annotation being drawn by the current drag.
    pending: Option<Annotation>,
    drag: Drag,
    selected: Option<u32>,
    changed_since_copy: bool,
    /// Bumped whenever the committed annotations change, so the window can cache its rendering.
    revision: u64,
}

impl EditorState {
    pub fn new(image: Bitmap, scale: f64) -> Self {
        Self {
            image,
            annotations: Vec::new(),
            history: History::default(),
            next_id: 1,
            tool: Tool::Arrow,
            color: 0,
            scale,
            pending: None,
            drag: Drag::None,
            selected: None,
            changed_since_copy: false,
            revision: 0,
        }
    }

    pub fn image(&self) -> &Bitmap {
        &self.image
    }

    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    pub fn pending(&self) -> Option<&Annotation> {
        self.pending.as_ref()
    }

    pub fn selected(&self) -> Option<u32> {
        self.selected
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn color_index(&self) -> usize {
        self.color
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// Whether the image changed since it was last copied (so closing should copy it again).
    pub fn changed_since_copy(&self) -> bool {
        self.changed_since_copy
    }

    pub fn mark_copied(&mut self) {
        self.changed_since_copy = false;
    }

    /// Follows the display scale when the window moves to another monitor.
    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    fn hit_tolerance(&self) -> f64 {
        4.0 * self.scale
    }

    /// Radius of the drag handles, in image pixels.
    fn handle_radius(&self) -> f64 {
        5.0 * self.scale
    }

    /// Blocks large enough that text inside cannot be read, scaled with display DPI.
    fn pixel_block_size(&self) -> i32 {
        round_like_dotnet(9.0 * self.scale).max(8.0) as i32
    }

    /// Each click counts up; undoing a marker gives its number back.
    fn next_number(&self) -> u32 {
        let highest = self.annotations.iter().filter_map(|a| match &a.shape {
            Shape::Number(n) => Some(n.number),
            _ => None,
        });
        highest.max().unwrap_or(0) + 1
    }

    // ---- Mouse ---------------------------------------------------------------------------

    pub fn mouse_down(&mut self, p: Point) -> Changes {
        if self.is_dragging() {
            return Changes::NONE;
        }

        if let Some(id) = self.selected
            && let Some(index) = self.handle_at(id, p)
        {
            let original = self.handles_of(id);
            self.drag = Drag::Handle { index, original };
            return Changes::NONE;
        }

        // Clicking an existing arrow or number selects it and starts moving it, whatever the tool.
        if let Some(id) = self.annotation_at(p) {
            self.selected = Some(id);
            let original = self.handles_of(id);
            self.drag = Drag::Move { start: p, original };
            return Changes::IMAGE;
        }

        self.selected = None;
        let color = PALETTE[self.color];
        let shape = match self.tool {
            Tool::Arrow => Shape::Arrow(Arrow::new(p, color, round_like_dotnet(7.0 * self.scale))),
            Tool::Number => Shape::Number(Number::new(
                p,
                self.next_number(),
                color,
                round_like_dotnet(26.0 * self.scale),
            )),
            Tool::Pixelate => Shape::Pixelate(Pixelate::new(
                p,
                self.pixel_block_size(),
                fastrand::i32(0..i32::MAX),
            )),
        };
        self.pending = Some(Annotation {
            id: self.next_id,
            shape,
        });
        self.next_id += 1;
        self.drag = Drag::Create;
        Changes::IMAGE
    }

    pub fn mouse_move(&mut self, p: Point) -> Changes {
        match self.drag.clone() {
            Drag::None => Changes::NONE,
            Drag::Move { start, original } => {
                let delta = self.clamp_move(&original, p - start);
                let moved: Vec<Point> = original.iter().map(|&h| h + delta).collect();
                self.set_selected_handles(&moved);
                Changes::IMAGE
            }
            Drag::Handle { index, original } => {
                let mut handles = original;
                handles[index] = self.clamp_to_image(p);
                self.set_selected_handles(&handles);
                Changes::IMAGE
            }
            Drag::Create => {
                let p = self.clamp_to_image(p);
                let image = &self.image;
                match self.pending.as_mut().map(|a| &mut a.shape) {
                    Some(Shape::Arrow(arrow)) => arrow.end = p,
                    Some(Shape::Pixelate(pixelate)) => pixelate.update(p, image),
                    // Drag to fine-tune the position.
                    Some(Shape::Number(number)) => number.center = p,
                    None => return Changes::NONE,
                }
                Changes::IMAGE
            }
        }
    }

    /// Ends the current drag (mouse up, or the mouse capture was lost).
    pub fn finish_drag(&mut self) -> Changes {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::None => Changes::NONE,
            Drag::Move { original, .. } | Drag::Handle { original, .. } => {
                if let Some(id) = self.selected {
                    let current = self.handles_of(id);
                    if current != original {
                        self.history.record_reshape(id, original, current);
                        self.committed();
                    }
                }
                Changes::ALL
            }
            Drag::Create => {
                if let Some(finished) = self.pending.take()
                    && !finished.is_empty()
                {
                    self.history.add(&mut self.annotations, finished);
                    self.committed();
                }
                Changes::ALL
            }
        }
    }

    pub fn cursor_at(&self, p: Point) -> Cursor {
        if let Some(id) = self.selected
            && self.handle_at(id, p).is_some()
        {
            Cursor::Hand
        } else if self.annotation_at(p).is_some() {
            Cursor::Move
        } else {
            Cursor::Cross
        }
    }

    // ---- Selection -----------------------------------------------------------------------

    /// The topmost arrow or number under `p`.
    fn annotation_at(&self, p: Point) -> Option<u32> {
        let tolerance = self.hit_tolerance();
        self.annotations
            .iter()
            .rev()
            .find(|a| a.is_editable() && a.hit_test(p, tolerance))
            .map(|a| a.id)
    }

    /// Index of the arrow end point under `p`, if any (a number's single handle doesn't count).
    fn handle_at(&self, id: u32, p: Point) -> Option<usize> {
        let handles = self.handles_of(id);
        if handles.len() < 2 {
            return None;
        }
        let reach = self.handle_radius() + self.hit_tolerance();
        handles.iter().position(|&h| (h - p).length() <= reach)
    }

    fn handles_of(&self, id: u32) -> Vec<Point> {
        self.annotations
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.handles())
            .unwrap_or_default()
    }

    fn set_selected_handles(&mut self, handles: &[Point]) {
        if let Some(id) = self.selected
            && let Some(annotation) = self.annotations.iter_mut().find(|a| a.id == id)
        {
            annotation.set_handles(handles);
        }
    }

    fn deselect(&mut self) -> Changes {
        if self.selected.take().is_some() {
            Changes::IMAGE
        } else {
            Changes::NONE
        }
    }

    // ---- Commands ------------------------------------------------------------------------

    /// Escape: deselects. Returns `None` when nothing was selected, meaning "close the editor".
    pub fn escape(&mut self) -> Option<Changes> {
        self.selected.is_some().then(|| self.deselect())
    }

    pub fn delete_selected(&mut self) -> Changes {
        let Some(id) = self.selected.take() else {
            return Changes::NONE;
        };
        self.history.remove(&mut self.annotations, id);
        self.committed();
        Changes::ALL
    }

    pub fn undo(&mut self) -> Changes {
        if self.history.undo(&mut self.annotations) {
            self.after_history_change()
        } else {
            Changes::NONE
        }
    }

    pub fn redo(&mut self) -> Changes {
        if self.history.redo(&mut self.annotations) {
            self.after_history_change()
        } else {
            Changes::NONE
        }
    }

    fn after_history_change(&mut self) -> Changes {
        // The selected annotation may just have been removed again.
        if let Some(id) = self.selected
            && !self.annotations.iter().any(|a| a.id == id)
        {
            self.selected = None;
        }
        self.committed();
        Changes::ALL
    }

    pub fn select_tool(&mut self, tool: Tool) -> Changes {
        let changed = self.tool != tool;
        self.tool = tool;
        let toolbar = if changed {
            Changes::TOOLBAR
        } else {
            Changes::NONE
        };
        toolbar.or(self.deselect())
    }

    pub fn select_color(&mut self, index: usize) -> Changes {
        if index >= PALETTE.len() {
            return Changes::NONE;
        }
        self.color = index;
        // Pixelate has no colour, so picking one switches back to the arrow.
        let tool = if self.tool == Tool::Pixelate {
            self.select_tool(Tool::Arrow)
        } else {
            Changes::NONE
        };
        Changes::TOOLBAR.or(tool)
    }

    // ---- Rendering -----------------------------------------------------------------------

    /// The screenshot with all committed annotations, at full size (for copying and saving).
    pub fn flatten(&self) -> Bitmap {
        render::flatten(&self.image, &self.annotations)
    }

    /// The annotation that changes with every mouse move, if any: the selected one while it is
    /// being moved. The window caches everything else ([`Self::render_base`]) during the drag.
    pub fn live_id(&self) -> Option<u32> {
        match self.drag {
            Drag::Move { .. } | Drag::Handle { .. } => self.selected,
            _ => None,
        }
    }

    /// The screenshot with the committed annotations except `exclude`.
    pub fn render_base(&self, exclude: Option<u32>) -> Bitmap {
        render::flatten(
            &self.image,
            self.annotations.iter().filter(|a| Some(a.id) != exclude),
        )
    }

    /// Draws what goes on top of [`Self::render_base`]: the annotation being moved or drawn and
    /// the selection outline. The selection is editor-only and never exported.
    pub fn render_overlay(&self, target: &mut Bitmap) {
        let live = self
            .live_id()
            .and_then(|id| self.annotations.iter().find(|a| a.id == id));
        render::draw_annotations(target, live.into_iter().chain(self.pending.as_ref()));

        let Some(selected) = self
            .selected
            .and_then(|id| self.annotations.iter().find(|a| a.id == id))
        else {
            return;
        };
        let (width, height) = (target.width(), target.height());
        if let Some(mut pixmap) = PixmapMut::from_bytes(target.pixels_mut(), width, height) {
            self.draw_selection(&mut pixmap, selected);
        }
    }

    /// Whether there is anything for [`Self::render_overlay`] to draw.
    pub fn has_overlay(&self) -> bool {
        self.pending.is_some() || self.selected.is_some()
    }

    fn draw_selection(&self, pixmap: &mut PixmapMut<'_>, selected: &Annotation) {
        let r = self.handle_radius();
        let bounds = selected.bounds().inflate(r, r);
        let mut paint = Paint::default();
        paint.set_color(draw::color(ACCENT.0, ACCENT.1, ACCENT.2, 255));

        // Dashed outline; WPF dash lengths are in multiples of the pen width.
        let width = (r * 0.35) as f32;
        if let Some(path) = draw::rounded_rect(
            bounds.x as f32,
            bounds.y as f32,
            bounds.width as f32,
            bounds.height as f32,
            r as f32,
        ) {
            let stroke = Stroke {
                width,
                dash: StrokeDash::new(vec![3.0 * width, 2.0 * width], 0.0),
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        // A single handle is just the centre of a number; only arrows get draggable end points.
        let handles = selected.handles().unwrap_or_default();
        if handles.len() < 2 {
            return;
        }
        let ring = Stroke {
            width: (r * 0.45) as f32,
            ..Stroke::default()
        };
        let mut white = Paint::default();
        white.set_color(draw::color(255, 255, 255, 255));
        for h in handles {
            if let Some(circle) = PathBuilder::from_circle(h.x as f32, h.y as f32, r as f32) {
                pixmap.fill_path(
                    &circle,
                    &white,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
                pixmap.stroke_path(&circle, &paint, &ring, Transform::identity(), None);
            }
        }
    }

    // ---- Helpers -------------------------------------------------------------------------

    fn committed(&mut self) {
        self.changed_since_copy = true;
        self.revision += 1;
    }

    fn clamp_to_image(&self, p: Point) -> Point {
        Point::new(
            p.x.clamp(0.0, self.image.width() as f64),
            p.y.clamp(0.0, self.image.height() as f64),
        )
    }

    /// Limits a move so every handle stays on the image, keeping the shape intact.
    fn clamp_move(&self, handles: &[Point], delta: Vector) -> Vector {
        let min_x = handles.iter().map(|h| h.x).fold(f64::INFINITY, f64::min);
        let max_x = handles
            .iter()
            .map(|h| h.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = handles.iter().map(|h| h.y).fold(f64::INFINITY, f64::min);
        let max_y = handles
            .iter()
            .map(|h| h.y)
            .fold(f64::NEG_INFINITY, f64::max);
        let (w, h) = (self.image.width() as f64, self.image.height() as f64);
        // Like Math.Clamp, but never panics if a handle somehow lies outside the image.
        let clamp = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi.max(lo));
        Vector::new(
            clamp(delta.x, -min_x, w - max_x),
            clamp(delta.y, -min_y, h - max_y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> EditorState {
        EditorState::new(Bitmap::new(200, 100), 1.0)
    }

    fn drag(editor: &mut EditorState, from: (f64, f64), to: (f64, f64)) {
        editor.mouse_down(Point::new(from.0, from.1));
        editor.mouse_move(Point::new(to.0, to.1));
        editor.finish_drag();
    }

    fn click(editor: &mut EditorState, at: (f64, f64)) {
        drag(editor, at, at);
    }

    fn arrow(editor: &EditorState, index: usize) -> &Arrow {
        match &editor.annotations()[index].shape {
            Shape::Arrow(arrow) => arrow,
            other => panic!("not an arrow: {other:?}"),
        }
    }

    fn numbers(editor: &EditorState) -> Vec<u32> {
        editor
            .annotations()
            .iter()
            .filter_map(|a| match &a.shape {
                Shape::Number(n) => Some(n.number),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn dragging_adds_an_arrow() {
        let mut e = editor();
        drag(&mut e, (10.0, 10.0), (80.0, 60.0));
        assert_eq!(e.annotations().len(), 1);
        assert!(e.changed_since_copy());
        assert!(e.can_undo());
        assert!(e.pending().is_none());
        assert_eq!(e.selected(), None);
    }

    #[test]
    fn a_click_adds_no_arrow() {
        let mut e = editor();
        drag(&mut e, (10.0, 10.0), (12.0, 11.0));
        assert!(e.annotations().is_empty());
        assert!(!e.changed_since_copy());
    }

    #[test]
    fn arrow_end_stays_on_the_image() {
        let mut e = editor();
        e.mouse_down(Point::new(10.0, 10.0));
        e.mouse_move(Point::new(500.0, -40.0));
        let Some(Annotation {
            shape: Shape::Arrow(arrow),
            ..
        }) = e.pending()
        else {
            panic!()
        };
        assert_eq!(arrow.end, Point::new(200.0, 0.0));
    }

    #[test]
    fn sizes_follow_the_display_scale_like_dotnet() {
        let mut e = EditorState::new(Bitmap::new(200, 100), 1.5);
        e.mouse_down(Point::new(10.0, 10.0));
        let Some(Annotation {
            shape: Shape::Arrow(arrow),
            ..
        }) = e.pending()
        else {
            panic!()
        };
        assert_eq!(arrow.thickness, 10.0); // Math.Round(10.5) = 10
        assert_eq!(e.pixel_block_size(), 14); // Math.Round(13.5) = 14
        assert_eq!(
            EditorState::new(Bitmap::new(1, 1), 1.0).pixel_block_size(),
            9
        );
        assert_eq!(
            EditorState::new(Bitmap::new(1, 1), 0.5).pixel_block_size(),
            8
        );
    }

    #[test]
    fn undo_and_redo() {
        let mut e = editor();
        drag(&mut e, (10.0, 10.0), (80.0, 60.0));
        e.mark_copied();
        assert_eq!(e.undo(), Changes::ALL);
        assert!(e.annotations().is_empty());
        assert!(e.changed_since_copy());
        assert_eq!(e.redo(), Changes::ALL);
        assert_eq!(e.annotations().len(), 1);
        assert_eq!(e.redo(), Changes::NONE);
    }

    #[test]
    fn numbers_count_up_and_come_back_after_undo() {
        let mut e = editor();
        e.select_tool(Tool::Number);
        click(&mut e, (20.0, 20.0));
        click(&mut e, (60.0, 20.0));
        click(&mut e, (100.0, 20.0));
        assert_eq!(numbers(&e), [1, 2, 3]);
        e.undo();
        click(&mut e, (140.0, 20.0));
        assert_eq!(numbers(&e), [1, 2, 3]);
    }

    #[test]
    fn dragging_a_new_number_moves_it() {
        let mut e = editor();
        e.select_tool(Tool::Number);
        drag(&mut e, (20.0, 20.0), (50.0, 60.0));
        assert_eq!(
            e.annotations()[0].handles().unwrap(),
            [Point::new(50.0, 60.0)]
        );
    }

    #[test]
    fn pixelate_needs_a_real_area_and_can_not_be_selected() {
        let mut e = editor();
        e.select_tool(Tool::Pixelate);
        drag(&mut e, (10.0, 10.0), (11.0, 40.0));
        assert!(e.annotations().is_empty());
        drag(&mut e, (10.0, 10.0), (60.0, 40.0));
        assert_eq!(e.annotations().len(), 1);
        // Clicking on it starts a new region instead of selecting it.
        e.mouse_down(Point::new(30.0, 20.0));
        assert_eq!(e.selected(), None);
        assert!(e.pending().is_some());
    }

    #[test]
    fn clicking_an_arrow_selects_and_moves_it_whatever_the_tool() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        e.select_tool(Tool::Number);
        drag(&mut e, (70.0, 51.0), (80.0, 61.0));
        assert_eq!(e.selected(), Some(e.annotations()[0].id));
        assert_eq!(numbers(&e), Vec::<u32>::new(), "no number was added");
        assert_eq!(arrow(&e, 0).start, Point::new(30.0, 60.0));
        assert_eq!(arrow(&e, 0).end, Point::new(130.0, 60.0));

        e.undo();
        assert_eq!(arrow(&e, 0).start, Point::new(20.0, 50.0));
    }

    #[test]
    fn moves_stay_on_the_image() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        drag(&mut e, (70.0, 50.0), (500.0, 500.0));
        assert_eq!(arrow(&e, 0).end, Point::new(200.0, 100.0));
        assert_eq!(arrow(&e, 0).start, Point::new(100.0, 100.0));
    }

    #[test]
    fn dragging_an_end_point_reshapes_the_selected_arrow() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        click(&mut e, (70.0, 50.0)); // select
        assert_eq!(e.cursor_at(Point::new(121.0, 52.0)), Cursor::Hand);
        drag(&mut e, (121.0, 52.0), (150.0, 90.0));
        assert_eq!(arrow(&e, 0).start, Point::new(20.0, 50.0));
        assert_eq!(arrow(&e, 0).end, Point::new(150.0, 90.0));
        e.undo();
        assert_eq!(arrow(&e, 0).end, Point::new(120.0, 50.0));
    }

    #[test]
    fn a_click_without_moving_records_nothing() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        e.mark_copied();
        click(&mut e, (70.0, 50.0));
        assert!(!e.changed_since_copy());
        e.undo();
        assert!(
            e.annotations().is_empty(),
            "the undo removed the arrow, not an empty move"
        );
    }

    #[test]
    fn delete_escape_and_undo() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        assert_eq!(e.delete_selected(), Changes::NONE, "nothing selected");
        click(&mut e, (70.0, 50.0));
        assert_eq!(e.escape(), Some(Changes::IMAGE));
        assert_eq!(e.escape(), None, "nothing selected: close");

        click(&mut e, (70.0, 50.0));
        e.delete_selected();
        assert!(e.annotations().is_empty());
        e.undo();
        assert_eq!(e.annotations().len(), 1);
        e.redo();
        assert!(e.annotations().is_empty());
    }

    #[test]
    fn undo_deselects_a_removed_annotation() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        click(&mut e, (70.0, 50.0));
        e.undo(); // nothing was moved, so this removes the arrow
        assert_eq!(e.selected(), None);
    }

    #[test]
    fn cursors() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        assert_eq!(e.cursor_at(Point::new(70.0, 52.0)), Cursor::Move);
        assert_eq!(e.cursor_at(Point::new(70.0, 90.0)), Cursor::Cross);
        assert_eq!(
            e.cursor_at(Point::new(121.0, 52.0)),
            Cursor::Move,
            "handles only when selected"
        );
    }

    #[test]
    fn colours_and_tools() {
        let mut e = editor();
        e.select_color(3);
        drag(&mut e, (10.0, 10.0), (80.0, 60.0));
        assert_eq!(arrow(&e, 0).color, PALETTE[3]);
        assert_eq!(e.select_tool(Tool::Arrow), Changes::NONE);

        e.select_tool(Tool::Pixelate);
        e.select_color(1);
        assert_eq!(
            e.tool(),
            Tool::Arrow,
            "a colour switches pixelate back to the arrow"
        );
    }

    #[test]
    fn choosing_a_tool_deselects() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        click(&mut e, (70.0, 50.0));
        assert_eq!(e.select_tool(Tool::Arrow), Changes::IMAGE);
        assert_eq!(e.selected(), None);
    }

    #[test]
    fn flatten_draws_the_annotations_but_not_the_selection() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (180.0, 50.0));
        let flat = e.flatten();
        assert_eq!(flat.pixel(100, 50), [0x30, 0x3B, 0xFF, 255]); // red, BGRA
        assert_eq!(e.image().pixel(100, 50), [0, 0, 0, 255]); // the original stays untouched

        click(&mut e, (100.0, 50.0));
        assert_eq!(e.flatten(), flat, "selection chrome is never exported");
        let mut view = e.render_base(None);
        e.render_overlay(&mut view);
        assert_ne!(view, flat, "but the editor shows it");
    }

    #[test]
    fn base_plus_overlay_matches_while_moving() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (120.0, 50.0));
        e.mouse_down(Point::new(70.0, 50.0));
        e.mouse_move(Point::new(75.0, 60.0));
        assert_eq!(e.live_id(), Some(e.annotations()[0].id));
        let base = e.render_base(e.live_id());
        assert_eq!(
            base,
            *e.image(),
            "the moving arrow is left out of the cached base"
        );
    }

    #[test]
    fn keyboard_shortcuts() {
        assert_eq!(
            command_for_key(b'Z' as u16, true, false),
            Some(Command::Undo)
        );
        assert_eq!(
            command_for_key(b'Z' as u16, true, true),
            Some(Command::Redo)
        );
        assert_eq!(
            command_for_key(b'Y' as u16, true, false),
            Some(Command::Redo)
        );
        assert_eq!(
            command_for_key(b'C' as u16, true, false),
            Some(Command::Copy)
        );
        assert_eq!(
            command_for_key(b'S' as u16, true, false),
            Some(Command::Save)
        );
        assert_eq!(
            command_for_key(b'A' as u16, false, false),
            Some(Command::Tool(Tool::Arrow))
        );
        assert_eq!(
            command_for_key(b'P' as u16, false, false),
            Some(Command::Tool(Tool::Pixelate))
        );
        assert_eq!(
            command_for_key(b'B' as u16, false, false),
            Some(Command::Tool(Tool::Pixelate))
        );
        assert_eq!(
            command_for_key(b'N' as u16, false, false),
            Some(Command::Tool(Tool::Number))
        );
        assert_eq!(
            command_for_key(b'3' as u16, false, false),
            Some(Command::Color(2))
        );
        assert_eq!(command_for_key(b'3' as u16, true, false), None);
        assert_eq!(
            command_for_key(vk::ESCAPE, false, false),
            Some(Command::Escape)
        );
        assert_eq!(command_for_key(0x2E, false, false), Some(Command::Delete));
        assert_eq!(command_for_key(0x08, false, false), Some(Command::Delete));
        assert_eq!(command_for_key(b'Q' as u16, false, false), None);
    }
}
