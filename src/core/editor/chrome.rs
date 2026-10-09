//! Layout and drawing of the editor window around the image: toolbar, status bar and canvas
//! (port of `EditorWindow.xaml` and the styles in `Theme/Styles.xaml`). Sizes are in DIPs and
//! scaled by the display scale; everything returned is in physical client pixels.
//!
//! Text is not drawn here: the window draws it with the system's text renderer at the rectangles
//! this module gives out.

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Stroke, Transform,
};

use super::{PALETTE, Rgb, Tool};
use crate::draw;
use crate::geometry::{IRect, Point, Rect};

// Theme colours.
pub const WINDOW_BACKGROUND: Rgb = Rgb(0x1C, 0x1C, 0x1E);
pub const TOOLBAR_BACKGROUND: Rgb = Rgb(0x2C, 0x2C, 0x2E);
pub const CONTROL_BACKGROUND: Rgb = Rgb(0x3A, 0x3A, 0x3C);
pub const HOVER_BACKGROUND: Rgb = Rgb(0x48, 0x48, 0x4A);
pub const ACCENT: Rgb = Rgb(0x0A, 0x84, 0xFF);
pub const FOREGROUND: Rgb = Rgb(0xF2, 0xF2, 0xF7);
pub const SUBTLE_FOREGROUND: Rgb = Rgb(0x8E, 0x8E, 0x93);
pub const SEPARATOR: Rgb = Rgb(0x48, 0x48, 0x4A);

/// Opacity of disabled buttons.
pub const DISABLED_OPACITY: f32 = 0.35;

// Sizes in DIPs.
const TOOLBAR_PADDING: (f64, f64) = (10.0, 7.0);
const BUTTON_SIZE: f64 = 34.0;
const BUTTON_MARGIN: f64 = 2.0;
const BUTTON_RADIUS: f64 = 7.0;
const SWATCH_SIZE: f64 = 24.0;
const SEPARATOR_MARGIN: f64 = 8.0;
const SEPARATOR_HEIGHT: f64 = 20.0;
const STATUS_HEIGHT: f64 = 26.0;
const STATUS_PADDING_X: f64 = 12.0;
const CANVAS_PADDING: f64 = 16.0;
const ICON_SIZE: f64 = 18.0;
/// Minimum window size (outer, like WPF's MinWidth/MinHeight).
pub const MIN_WINDOW_SIZE: (f64, f64) = (580.0, 260.0);
/// The window may take at most this share of the monitor's work area.
pub const MAX_WORK_AREA_SHARE: f64 = 0.92;

fn toolbar_height() -> f64 {
    2.0 * TOOLBAR_PADDING.1 + BUTTON_SIZE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Tool(Tool),
    Color(usize),
    Undo,
    Redo,
    Copy,
    Save,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub scale: f64,
    pub client: (i32, i32),
    pub toolbar: IRect,
    pub status: IRect,
    pub buttons: Vec<(Button, IRect)>,
    pub separators: Vec<IRect>,
    /// Where the image is shown, in client pixels (may be scaled down).
    pub image: Rect,
    /// Display size / image size (≤ 1).
    pub zoom: f64,
    /// The thin frame around the image.
    pub border: IRect,
}

/// The client size that shows the image 1:1 (if it fits within `max_client`), at least
/// `min_client`.
pub fn preferred_client_size(
    image: (u32, u32),
    scale: f64,
    min_client: (i32, i32),
    max_client: (i32, i32),
) -> (i32, i32) {
    let border = border_width(scale);
    let pad = px(CANVAS_PADDING, scale);
    let chrome_w = 2 * pad + 2 * border;
    let chrome_h = px(toolbar_height(), scale) + px(STATUS_HEIGHT, scale) + 2 * pad + 2 * border;
    let width = (image.0 as i32 + chrome_w)
        .min(max_client.0)
        .max(min_client.0);
    let height = (image.1 as i32 + chrome_h)
        .min(max_client.1)
        .max(min_client.1);
    (width, height)
}

pub fn layout(client: (i32, i32), image: (u32, u32), scale: f64) -> Layout {
    let (width, height) = client;
    let toolbar = IRect::new(0, 0, width, px(toolbar_height(), scale));
    let status_height = px(STATUS_HEIGHT, scale);
    let status = IRect::new(0, height - status_height, width, status_height);

    let mut buttons = Vec::new();
    let mut separators = Vec::new();
    let button_y = TOOLBAR_PADDING.1;
    let mut x = TOOLBAR_PADDING.0;
    let button = |x: &mut f64, b: Button, buttons: &mut Vec<(Button, IRect)>| {
        *x += BUTTON_MARGIN;
        buttons.push((b, rect_px(*x, button_y, BUTTON_SIZE, BUTTON_SIZE, scale)));
        *x += BUTTON_SIZE + BUTTON_MARGIN;
    };
    let separator = |x: &mut f64, separators: &mut Vec<IRect>| {
        *x += SEPARATOR_MARGIN;
        let y = (toolbar_height() - SEPARATOR_HEIGHT) / 2.0;
        separators.push(rect_px(*x, y, 1.0, SEPARATOR_HEIGHT, scale));
        *x += 1.0 + SEPARATOR_MARGIN;
    };

    for tool in [Tool::Arrow, Tool::Pixelate, Tool::Number] {
        button(&mut x, Button::Tool(tool), &mut buttons);
    }
    separator(&mut x, &mut separators);
    for index in 0..PALETTE.len() {
        x += BUTTON_MARGIN;
        let y = (toolbar_height() - SWATCH_SIZE) / 2.0;
        buttons.push((
            Button::Color(index),
            rect_px(x, y, SWATCH_SIZE, SWATCH_SIZE, scale),
        ));
        x += SWATCH_SIZE + BUTTON_MARGIN;
    }
    separator(&mut x, &mut separators);
    button(&mut x, Button::Undo, &mut buttons);
    button(&mut x, Button::Redo, &mut buttons);

    // Copy and Save on the right.
    let mut right = width as f64 / scale - TOOLBAR_PADDING.0;
    for b in [Button::Save, Button::Copy] {
        right -= BUTTON_MARGIN + BUTTON_SIZE;
        buttons.push((b, rect_px(right, button_y, BUTTON_SIZE, BUTTON_SIZE, scale)));
        right -= BUTTON_MARGIN;
    }

    // Canvas: padding, a 1 px frame, then the image scaled down to fit (never up).
    let pad = px(CANVAS_PADDING, scale);
    let border = border_width(scale);
    let inner = IRect::new(
        pad,
        toolbar.bottom() + pad,
        width - 2 * pad,
        status.y - toolbar.bottom() - 2 * pad,
    );
    let available = (
        (inner.width - 2 * border).max(1) as f64,
        (inner.height - 2 * border).max(1) as f64,
    );
    let zoom = (available.0 / image.0 as f64)
        .min(available.1 / image.1 as f64)
        .min(1.0);
    let (shown_w, shown_h) = (image.0 as f64 * zoom, image.1 as f64 * zoom);
    let image_rect = Rect::new(
        (inner.x as f64 + (inner.width as f64 - shown_w) / 2.0).round(),
        (inner.y as f64 + (inner.height as f64 - shown_h) / 2.0).round(),
        shown_w,
        shown_h,
    );
    let border_rect = IRect::new(
        image_rect.x as i32 - border,
        image_rect.y as i32 - border,
        shown_w.round() as i32 + 2 * border,
        shown_h.round() as i32 + 2 * border,
    );

    Layout {
        scale,
        client,
        toolbar,
        status,
        buttons,
        separators,
        image: image_rect,
        zoom,
        border: border_rect,
    }
}

impl Layout {
    pub fn button_at(&self, p: (i32, i32)) -> Option<Button> {
        let point = Point::new(p.0 as f64, p.1 as f64);
        self.buttons
            .iter()
            .find(|(_, r)| contains(*r, point))
            .map(|(b, _)| *b)
    }

    pub fn button_rect(&self, button: Button) -> Option<IRect> {
        self.buttons
            .iter()
            .find(|(b, _)| *b == button)
            .map(|(_, r)| *r)
    }

    pub fn image_contains(&self, p: (i32, i32)) -> bool {
        self.image
            .contains(Point::new(p.0 as f64 + 0.5, p.1 as f64 + 0.5))
    }

    /// Client pixel → image pixel.
    pub fn to_image(&self, p: (i32, i32)) -> Point {
        Point::new(
            (p.0 as f64 + 0.5 - self.image.x) / self.zoom,
            (p.1 as f64 + 0.5 - self.image.y) / self.zoom,
        )
    }

    /// The image area in whole client pixels.
    pub fn image_pixels(&self) -> IRect {
        IRect::new(
            self.image.x as i32,
            self.image.y as i32,
            self.image.width.round() as i32,
            self.image.height.round() as i32,
        )
    }

    /// Where the status bar's left text (the image size) goes.
    pub fn status_left(&self) -> IRect {
        let pad = px(STATUS_PADDING_X, self.scale);
        IRect::new(
            pad,
            self.status.y,
            self.status.width / 2,
            self.status.height,
        )
    }

    /// Where the status bar's right text (hint or last action) goes.
    pub fn status_right(&self) -> IRect {
        let pad = px(STATUS_PADDING_X, self.scale);
        IRect::new(
            pad,
            self.status.y,
            self.status.width - 2 * pad,
            self.status.height,
        )
    }
}

/// What the toolbar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarState {
    pub tool: Tool,
    pub color: usize,
    pub can_undo: bool,
    pub can_redo: bool,
    pub hover: Option<Button>,
}

impl ToolbarState {
    pub fn is_enabled(&self, button: Button) -> bool {
        match button {
            Button::Undo => self.can_undo,
            Button::Redo => self.can_redo,
            Button::Tool(tool) => super::state::EditorState::tool_available(tool),
            _ => true,
        }
    }
}

/// Draws the window background, toolbar, status bar and image frame. The image and texts come
/// on top of this.
pub fn draw(pixmap: &mut PixmapMut<'_>, layout: &Layout, toolbar: &ToolbarState) {
    let s = layout.scale;
    pixmap.fill(color(WINDOW_BACKGROUND, 1.0));
    fill_rect(pixmap, layout.toolbar, TOOLBAR_BACKGROUND);
    fill_rect(pixmap, layout.status, TOOLBAR_BACKGROUND);
    fill_rect(pixmap, layout.border, CONTROL_BACKGROUND);
    for separator in &layout.separators {
        fill_rect(pixmap, *separator, SEPARATOR);
    }

    for &(button, rect) in &layout.buttons {
        let enabled = toolbar.is_enabled(button);
        let hover = enabled && toolbar.hover == Some(button);
        match button {
            Button::Color(index) => draw_swatch(
                pixmap,
                rect,
                PALETTE[index],
                index == toolbar.color,
                hover,
                s,
            ),
            _ => {
                let checked = button == Button::Tool(toolbar.tool);
                let background = if checked {
                    Some(ACCENT)
                } else if hover {
                    Some(HOVER_BACKGROUND)
                } else {
                    None
                };
                if let Some(background) = background {
                    let radius = (BUTTON_RADIUS * s) as f32;
                    fill_path(pixmap, rounded(rect, radius), color(background, 1.0));
                }
                let opacity = if enabled { 1.0 } else { DISABLED_OPACITY };
                draw_icon(pixmap, button, rect, opacity, s);
            }
        }
    }
}

fn draw_swatch(
    pixmap: &mut PixmapMut<'_>,
    rect: IRect,
    rgb: Rgb,
    checked: bool,
    hover: bool,
    s: f64,
) {
    let (cx, cy) = (
        rect.x as f32 + rect.width as f32 / 2.0,
        rect.y as f32 + rect.height as f32 / 2.0,
    );
    if let Some(circle) = PathBuilder::from_circle(cx, cy, (7.0 * s) as f32) {
        fill_path(pixmap, Some(circle), color(rgb, 1.0));
    }
    // The ring: full when chosen, half when hovered (hover wins, as in the WPF template).
    let ring = if hover {
        0.5
    } else if checked {
        1.0
    } else {
        return;
    };
    let width = (2.0 * s) as f32;
    if let Some(circle) = PathBuilder::from_circle(cx, cy, (11.0 * s) as f32 - width / 2.0) {
        let stroke = Stroke {
            width,
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &circle,
            &paint(color(Rgb(255, 255, 255), ring)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// The toolbar icons, from the XAML path data (24×24 design grid).
fn draw_icon(pixmap: &mut PixmapMut<'_>, button: Button, rect: IRect, opacity: f32, s: f64) {
    let fg = color(FOREGROUND, opacity);
    let (cx, cy) = (
        rect.x as f64 + rect.width as f64 / 2.0,
        rect.y as f64 + rect.height as f64 / 2.0,
    );
    let stroke = Stroke {
        width: (2.0 * s) as f32,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };

    if button == Button::Tool(Tool::Number) {
        // A 20×20 rounded square; the "1" inside is text, drawn by the window.
        let half = 10.0 * s - s; // inset by half the stroke
        let r = IRect::new(
            (cx - half) as i32,
            (cy - half) as i32,
            (2.0 * half) as i32,
            (2.0 * half) as i32,
        );
        if let Some(path) = draw::rounded_rect(
            r.x as f32,
            r.y as f32,
            r.width as f32,
            r.height as f32,
            (4.0 * s) as f32,
        ) {
            pixmap.stroke_path(&path, &paint(fg), &stroke, Transform::identity(), None);
        }
        return;
    }

    let (path, filled) = icon_path(button);
    let Some(path) = path else { return };
    // Stretch="Uniform" into an 18×18 box: the path's own bounds are scaled to fit and centred.
    let bounds = path.bounds();
    let factor = (ICON_SIZE * s) as f32 / bounds.width().max(bounds.height());
    let transform = Transform::from_translate(cx as f32, cy as f32)
        .pre_scale(factor, factor)
        .pre_translate(
            -(bounds.left() + bounds.width() / 2.0),
            -(bounds.top() + bounds.height() / 2.0),
        );
    if filled {
        pixmap.fill_path(&path, &paint(fg), FillRule::Winding, transform, None);
    } else {
        // Transform the path first so the stroke width isn't scaled with it.
        if let Some(path) = path.transform(transform) {
            pixmap.stroke_path(&path, &paint(fg), &stroke, Transform::identity(), None);
        }
    }
}

/// The icon outline in the 24×24 design grid, and whether it is filled (otherwise stroked).
fn icon_path(button: Button) -> (Option<Path>, bool) {
    let mut pb = PathBuilder::new();
    fn polyline(pb: &mut PathBuilder, points: &[(f32, f32)]) {
        pb.move_to(points[0].0, points[0].1);
        for &(x, y) in &points[1..] {
            pb.line_to(x, y);
        }
    }
    let filled = match button {
        // M5,19 L19,5 M10,5 L19,5 L19,14
        Button::Tool(Tool::Arrow) => {
            polyline(&mut pb, &[(5.0, 19.0), (19.0, 5.0)]);
            polyline(&mut pb, &[(10.0, 5.0), (19.0, 5.0), (19.0, 14.0)]);
            false
        }
        // Five filled squares in a checkerboard.
        Button::Tool(Tool::Pixelate) => {
            for (x, y) in [
                (3.0, 3.0),
                (15.0, 3.0),
                (9.0, 9.0),
                (3.0, 15.0),
                (15.0, 15.0),
            ] {
                if let Some(r) = tiny_skia::Rect::from_xywh(x, y, 6.0, 6.0) {
                    pb.push_rect(r);
                }
            }
            true
        }
        // M9,5 L4,10 L9,15 M4,10 H15 A5,5 0 0 1 15,20 H11
        Button::Undo => {
            polyline(&mut pb, &[(9.0, 5.0), (4.0, 10.0), (9.0, 15.0)]);
            pb.move_to(4.0, 10.0);
            pb.line_to(15.0, 10.0);
            half_circle(&mut pb, (15.0, 15.0), 5.0, 1.0);
            pb.line_to(11.0, 20.0);
            false
        }
        // M15,5 L20,10 L15,15 M20,10 H9 A5,5 0 0 0 9,20 H13
        Button::Redo => {
            polyline(&mut pb, &[(15.0, 5.0), (20.0, 10.0), (15.0, 15.0)]);
            pb.move_to(20.0, 10.0);
            pb.line_to(9.0, 10.0);
            half_circle(&mut pb, (9.0, 15.0), 5.0, -1.0);
            pb.line_to(13.0, 20.0);
            false
        }
        // M9,9 H20 V20 H9 Z M5,15 H4 V4 H15 V5
        Button::Copy => {
            polyline(
                &mut pb,
                &[(9.0, 9.0), (20.0, 9.0), (20.0, 20.0), (9.0, 20.0)],
            );
            pb.close();
            polyline(
                &mut pb,
                &[
                    (5.0, 15.0),
                    (4.0, 15.0),
                    (4.0, 4.0),
                    (15.0, 4.0),
                    (15.0, 5.0),
                ],
            );
            false
        }
        // M12,4 V15 M7,10 L12,15 L17,10 M4,17 V20 H20 V17
        Button::Save => {
            polyline(&mut pb, &[(12.0, 4.0), (12.0, 15.0)]);
            polyline(&mut pb, &[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            polyline(
                &mut pb,
                &[(4.0, 17.0), (4.0, 20.0), (20.0, 20.0), (20.0, 17.0)],
            );
            false
        }
        _ => return (None, false),
    };
    (pb.finish(), filled)
}

/// Half a circle from the top of `center` to the bottom, bulging right (`side` 1) or left (-1).
fn half_circle(pb: &mut PathBuilder, center: (f32, f32), r: f32, side: f32) {
    let k = r * 0.552_284_8;
    let (cx, cy) = center;
    pb.cubic_to(
        cx + side * k,
        cy - r,
        cx + side * r,
        cy - k,
        cx + side * r,
        cy,
    );
    pb.cubic_to(cx + side * r, cy + k, cx + side * k, cy + r, cx, cy + r);
}

fn px(dip: f64, scale: f64) -> i32 {
    (dip * scale).round() as i32
}

fn rect_px(x: f64, y: f64, w: f64, h: f64, scale: f64) -> IRect {
    let (x0, y0) = (px(x, scale), px(y, scale));
    IRect::new(x0, y0, px(x + w, scale) - x0, px(y + h, scale) - y0)
}

fn border_width(scale: f64) -> i32 {
    px(1.0, scale).max(1)
}

fn contains(r: IRect, p: Point) -> bool {
    p.x >= r.x as f64 && p.x < r.right() as f64 && p.y >= r.y as f64 && p.y < r.bottom() as f64
}

fn color(rgb: Rgb, opacity: f32) -> tiny_skia::Color {
    draw::color(rgb.0, rgb.1, rgb.2, (opacity * 255.0).round() as u8)
}

fn paint(c: tiny_skia::Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(c);
    paint
}

fn fill_rect(pixmap: &mut PixmapMut<'_>, r: IRect, rgb: Rgb) {
    if let Some(rect) =
        tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.width as f32, r.height as f32)
    {
        pixmap.fill_rect(rect, &paint(color(rgb, 1.0)), Transform::identity(), None);
    }
}

fn rounded(r: IRect, radius: f32) -> Option<Path> {
    draw::rounded_rect(
        r.x as f32,
        r.y as f32,
        r.width as f32,
        r.height as f32,
        radius,
    )
}

fn fill_path(pixmap: &mut PixmapMut<'_>, path: Option<Path>, c: tiny_skia::Color) {
    if let Some(path) = path {
        pixmap.fill_path(
            &path,
            &paint(c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_images_are_shown_one_to_one() {
        let client = preferred_client_size((800, 500), 1.0, (560, 220), (1800, 1000));
        let l = layout(client, (800, 500), 1.0);
        assert_eq!(l.zoom, 1.0);
        assert_eq!((l.image.width, l.image.height), (800.0, 500.0));
        // Toolbar 48, padding 16, frame 1.
        assert_eq!((l.image.x, l.image.y), (17.0, 65.0));
        assert_eq!(client, (834, 500 + 48 + 26 + 34));
    }

    #[test]
    fn large_images_are_scaled_down_to_fit() {
        let client = preferred_client_size((3840, 2160), 1.0, (560, 220), (1766, 950));
        assert_eq!(client, (1766, 950));
        let l = layout(client, (3840, 2160), 1.0);
        assert!(l.zoom < 0.5);
        assert!(l.image.right() <= 1766.0 - 16.0 && l.image.bottom() <= l.status.y as f64 - 16.0);
        // Client → image coordinates map the corners onto the image.
        let bottom_right = l.to_image((l.image.right() as i32 - 1, l.image.bottom() as i32 - 1));
        // Within two screen pixels, which is 2/zoom image pixels.
        let tolerance = 2.0 / l.zoom;
        assert!(
            (bottom_right.x - 3840.0).abs() < tolerance,
            "{bottom_right:?}"
        );
        assert!(
            (bottom_right.y - 2160.0).abs() < tolerance,
            "{bottom_right:?}"
        );
        let top_left = l.to_image((l.image.x as i32, l.image.y as i32));
        assert!(
            top_left.x.abs() < tolerance && top_left.y.abs() < tolerance,
            "{top_left:?}"
        );
    }

    #[test]
    fn tiny_images_get_the_minimum_size_and_stay_centred() {
        let client = preferred_client_size((50, 40), 1.0, (560, 220), (1800, 1000));
        assert_eq!(client, (560, 220));
        let l = layout(client, (50, 40), 1.0);
        assert_eq!(l.zoom, 1.0);
        assert_eq!(l.image.x, 255.0);
    }

    #[test]
    fn toolbar_buttons_sit_where_the_xaml_puts_them() {
        let l = layout((900, 600), (100, 100), 1.0);
        assert_eq!(
            l.button_rect(Button::Tool(Tool::Arrow)),
            Some(IRect::new(12, 7, 34, 34))
        );
        assert_eq!(
            l.button_rect(Button::Tool(Tool::Number)),
            Some(IRect::new(88, 7, 34, 34))
        );
        // Separator: 124 + 8.
        assert_eq!(l.separators[0], IRect::new(132, 14, 1, 20));
        assert_eq!(
            l.button_rect(Button::Color(0)),
            Some(IRect::new(143, 12, 24, 24))
        );
        assert_eq!(
            l.button_rect(Button::Save),
            Some(IRect::new(854, 7, 34, 34))
        );
        assert_eq!(
            l.button_rect(Button::Copy),
            Some(IRect::new(816, 7, 34, 34))
        );
        assert_eq!(l.button_at((20, 20)), Some(Button::Tool(Tool::Arrow)));
        assert_eq!(l.button_at((5, 20)), None);
    }

    #[test]
    fn layout_scales_with_dpi() {
        let l = layout((1800, 1200), (100, 100), 2.0);
        assert_eq!(l.toolbar.height, 96);
        assert_eq!(
            l.button_rect(Button::Tool(Tool::Arrow)),
            Some(IRect::new(24, 14, 68, 68))
        );
    }

    #[test]
    fn draws_the_chrome() {
        let l = layout((600, 300), (100, 100), 1.0);
        let state = ToolbarState {
            tool: Tool::Arrow,
            color: 0,
            can_undo: false,
            can_redo: true,
            hover: None,
        };
        let mut pixels = vec![0u8; 600 * 300 * 4];
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 600, 300).unwrap();
        draw(&mut pixmap, &l, &state);
        let at = |x: usize, y: usize| -> [u8; 3] {
            pixels[(y * 600 + x) * 4..][..3].try_into().unwrap()
        };
        assert_eq!(at(300, 3), [0x2E, 0x2C, 0x2C], "toolbar (BGR)");
        assert_eq!(at(300, 295), [0x2E, 0x2C, 0x2C], "status bar");
        assert_eq!(at(5, 150), [0x1E, 0x1C, 0x1C], "window");
        assert_eq!(
            at(14, 20),
            [0xFF, 0x84, 0x0A],
            "checked arrow tool is accent blue"
        );
        let swatch = l.button_rect(Button::Color(0)).unwrap();
        let (sx, sy) = ((swatch.x + 12) as usize, (swatch.y + 12) as usize);
        assert_eq!(at(sx, sy), [0x30, 0x3B, 0xFF], "red swatch");
    }
}
