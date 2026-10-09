//! The editor's state machine (port of the logic in `EditorWindow.xaml.cs`), without any window:
//! the window feeds it mouse positions in image pixels and commands, and repaints what it says
//! changed.

use super::annotation::{Annotation, Shape};
use super::arrow::Arrow;
use super::history::History;
use super::render;
use super::{Bitmap, PALETTE, Tool, round_like_dotnet};
use crate::geometry::Point;
use crate::hotkey::vk;

/// What needs repainting after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changes {
    /// The image area (annotations or the one being drawn).
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
}

/// Something the user asked for with the keyboard or the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Close,
    Undo,
    Redo,
    Copy,
    Save,
    Tool(Tool),
    Color(usize),
}

/// The editor's keyboard shortcuts.
pub fn command_for_key(key: u16, ctrl: bool, shift: bool) -> Option<Command> {
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
        vk::ESCAPE => Command::Close,
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

pub struct EditorState {
    image: Bitmap,
    annotations: Vec<Annotation>,
    history: History,
    next_id: u32,
    tool: Tool,
    color: usize,
    /// Display scale when the editor opened (1.0 = 96 DPI); sizes annotations like the C# version.
    scale: f64,
    /// The annotation being drawn by the current drag.
    pending: Option<Annotation>,
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
        self.pending.is_some()
    }

    /// Whether the image changed since it was last copied (so closing should copy it again).
    pub fn changed_since_copy(&self) -> bool {
        self.changed_since_copy
    }

    pub fn mark_copied(&mut self) {
        self.changed_since_copy = false;
    }

    /// Follows the display scale when the window moves to another monitor; sizes new annotations.
    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Whether a tool is available yet. Pixelate and numbers arrive in phase 4.
    pub fn tool_available(tool: Tool) -> bool {
        tool == Tool::Arrow
    }

    // ---- Drawing -------------------------------------------------------------------------

    pub fn mouse_down(&mut self, p: Point) -> Changes {
        if self.pending.is_some() {
            return Changes::NONE;
        }
        let shape = match self.tool {
            Tool::Arrow => Shape::Arrow(Arrow::new(
                p,
                PALETTE[self.color],
                round_like_dotnet(7.0 * self.scale),
            )),
            Tool::Pixelate | Tool::Number => return Changes::NONE,
        };
        self.pending = Some(Annotation {
            id: self.next_id,
            shape,
        });
        self.next_id += 1;
        Changes::IMAGE
    }

    pub fn mouse_move(&mut self, p: Point) -> Changes {
        let p = self.clamp_to_image(p);
        match &mut self.pending {
            Some(Annotation {
                shape: Shape::Arrow(arrow),
                ..
            }) => {
                arrow.end = p;
                Changes::IMAGE
            }
            None => Changes::NONE,
        }
    }

    /// Ends the current drag (mouse up, or the mouse capture was lost).
    pub fn finish_drag(&mut self) -> Changes {
        let Some(finished) = self.pending.take() else {
            return Changes::NONE;
        };
        if !finished.is_empty() {
            self.history.add(&mut self.annotations, finished);
            self.committed();
        }
        Changes::ALL
    }

    // ---- Commands ------------------------------------------------------------------------

    pub fn undo(&mut self) -> Changes {
        if self.history.undo(&mut self.annotations) {
            self.committed();
            Changes::ALL
        } else {
            Changes::NONE
        }
    }

    pub fn redo(&mut self) -> Changes {
        if self.history.redo(&mut self.annotations) {
            self.committed();
            Changes::ALL
        } else {
            Changes::NONE
        }
    }

    pub fn select_tool(&mut self, tool: Tool) -> Changes {
        if !Self::tool_available(tool) || self.tool == tool {
            return Changes::NONE;
        }
        self.tool = tool;
        Changes::TOOLBAR
    }

    pub fn select_color(&mut self, index: usize) -> Changes {
        if index >= PALETTE.len() {
            return Changes::NONE;
        }
        self.color = index;
        // Pixelate has no colour, so picking one switches back to the arrow.
        if self.tool == Tool::Pixelate {
            self.tool = Tool::Arrow;
        }
        Changes::TOOLBAR
    }

    /// The screenshot with all committed annotations, at full size.
    pub fn flatten(&self) -> Bitmap {
        render::flatten(&self.image, &self.annotations)
    }

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

    #[test]
    fn dragging_adds_an_arrow() {
        let mut e = editor();
        drag(&mut e, (10.0, 10.0), (80.0, 60.0));
        assert_eq!(e.annotations().len(), 1);
        assert!(e.changed_since_copy());
        assert!(e.can_undo());
        assert!(e.pending().is_none());
    }

    #[test]
    fn a_click_adds_nothing() {
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
    fn arrow_size_follows_the_display_scale_like_dotnet() {
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
    fn colours_and_tools() {
        let mut e = editor();
        e.select_color(3);
        drag(&mut e, (10.0, 10.0), (80.0, 60.0));
        let Shape::Arrow(arrow) = &e.annotations()[0].shape;
        assert_eq!(arrow.color, PALETTE[3]);
        assert_eq!(e.select_tool(Tool::Arrow), Changes::NONE);
    }

    #[test]
    fn flatten_draws_the_annotations() {
        let mut e = editor();
        drag(&mut e, (20.0, 50.0), (180.0, 50.0));
        let flat = e.flatten();
        assert_eq!(flat.pixel(100, 50), [0x30, 0x3B, 0xFF, 255]); // red, BGRA
        assert_eq!(e.image().pixel(100, 50), [0, 0, 0, 255]); // the original stays untouched
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
            command_for_key(b'B' as u16, false, false),
            Some(Command::Tool(Tool::Pixelate))
        );
        assert_eq!(
            command_for_key(b'3' as u16, false, false),
            Some(Command::Color(2))
        );
        assert_eq!(command_for_key(b'3' as u16, true, false), None);
        assert_eq!(
            command_for_key(vk::ESCAPE, false, false),
            Some(Command::Close)
        );
        assert_eq!(command_for_key(b'Q' as u16, false, false), None);
    }
}
