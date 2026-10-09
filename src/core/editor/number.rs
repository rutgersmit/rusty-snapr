//! A numbered marker: the number in white (or dark on light colours) on a solid rounded square
//! that grows into a pill for two or more digits (port of `NumberAnnotation.cs`).

use std::sync::OnceLock;

use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont, point};
use tiny_skia::{FillRule, Paint, PixmapMut, Stroke, Transform};

use super::Rgb;
use crate::draw;
use crate::geometry::{Point, Rect};

/// Digits of Carlito Bold (OFL), used when Segoe UI Bold isn't available.
const FALLBACK_FONT: &[u8] = include_bytes!("../../../assets/fonts/snapr-digits-bold.ttf");

static FONT: OnceLock<FontArc> = OnceLock::new();

/// Sets the font for the numbers (Segoe UI Bold on Windows). Call once at startup; returns false
/// if the data isn't a usable font, in which case the built-in fallback is used.
pub fn set_font(data: Vec<u8>) -> bool {
    match FontArc::try_from_vec(data) {
        Ok(font) => FONT.set(font).is_ok(),
        Err(_) => false,
    }
}

fn font() -> &'static FontArc {
    FONT.get_or_init(|| FontArc::try_from_slice(FALLBACK_FONT).expect("built-in font"))
}

const DARK_TEXT: Rgb = Rgb(0x1C, 0x1C, 0x1E);

#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    pub center: Point,
    pub number: u32,
    pub color: Rgb,
    /// Height of the marker in image pixels.
    pub size: f64,
}

/// The number laid out in the marker's font: glyphs relative to the text's top-left corner.
struct TextLayout {
    glyphs: Vec<Glyph>,
    width: f32,
    height: f32,
}

impl Number {
    pub fn new(center: Point, number: u32, color: Rgb, size: f64) -> Self {
        Self {
            center,
            number,
            color,
            size,
        }
    }

    pub fn handles(&self) -> Vec<Point> {
        vec![self.center]
    }

    pub fn set_handles(&mut self, handles: &[Point]) {
        self.center = handles[0];
    }

    pub fn hit_test(&self, p: Point, tolerance: f64) -> bool {
        self.marker().inflate(tolerance, tolerance).contains(p)
    }

    pub fn bounds(&self) -> Rect {
        self.marker()
    }

    /// The rounded rectangle: square for one digit, wider for more.
    fn marker(&self) -> Rect {
        let width = self.size.max(self.text().width as f64 + self.size * 0.5);
        Rect::new(
            self.center.x - width / 2.0,
            self.center.y - self.size / 2.0,
            width,
            self.size,
        )
    }

    fn text(&self) -> TextLayout {
        let font = font();
        // WPF sizes text by its em height; ab_glyph's scale is the ascent-to-descent height.
        let em = (self.size * 0.58) as f32;
        let units_per_em = font.units_per_em().unwrap_or(1000.0);
        let scale = PxScale::from(em * font.height_unscaled() / units_per_em);
        let scaled = font.as_scaled(scale);

        let mut glyphs = Vec::new();
        let mut x = 0.0;
        let mut previous = None;
        for c in self.number.to_string().chars() {
            let id = font.glyph_id(c);
            if let Some(previous) = previous {
                x += scaled.kern(previous, id);
            }
            glyphs.push(id.with_scale_and_position(scale, point(x, scaled.ascent())));
            x += scaled.h_advance(id);
            previous = Some(id);
        }
        TextLayout {
            glyphs,
            width: x,
            height: scaled.height(),
        }
    }

    pub fn render(&self, pixmap: &mut PixmapMut<'_>, transform: Transform) {
        let marker = self.marker();
        let radius = (self.size * 0.28) as f32;
        let mut paint = Paint::default();

        let offset = (self.size * 0.06).max(1.0) as f32;
        let rect = |r: Rect| {
            draw::rounded_rect(
                r.x as f32,
                r.y as f32,
                r.width as f32,
                r.height as f32,
                radius,
            )
        };
        if let Some(path) = rect(marker) {
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
                width: (self.size * 0.07).max(1.5) as f32,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }

        let text_color = if is_light(self.color) {
            DARK_TEXT
        } else {
            Rgb(255, 255, 255)
        };
        let text = self.text();
        let left = self.center.x as f32 - text.width / 2.0 + transform.tx;
        let top = self.center.y as f32 - text.height / 2.0 + transform.ty;
        draw_glyphs(pixmap, &text.glyphs, left, top, text_color);
    }
}

/// Light colours (like yellow) get dark text.
fn is_light(c: Rgb) -> bool {
    0.299 * c.0 as f64 + 0.587 * c.1 as f64 + 0.114 * c.2 as f64 > 186.0
}

/// Blends anti-aliased glyphs onto an opaque BGRA pixmap.
fn draw_glyphs(pixmap: &mut PixmapMut<'_>, glyphs: &[Glyph], left: f32, top: f32, color: Rgb) {
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    let bgr = [color.2 as f32, color.1 as f32, color.0 as f32];
    for glyph in glyphs {
        let mut glyph = glyph.clone();
        glyph.position = point(glyph.position.x + left, glyph.position.y + top);
        let Some(outline) = font().outline_glyph(glyph) else {
            continue;
        };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            let x = bounds.min.x as i32 + gx as i32;
            let y = bounds.min.y as i32 + gy as i32;
            if x < 0 || y < 0 || x >= width || y >= height {
                return;
            }
            let i = (y as usize * width as usize + x as usize) * 4;
            let a = coverage.clamp(0.0, 1.0);
            for (channel, target) in bgr.iter().zip(&mut data[i..i + 3]) {
                *target = (channel * a + *target as f32 * (1.0 - a)).round() as u8;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(number: u32, color: Rgb) -> Number {
        Number::new(Point::new(50.0, 40.0), number, color, 26.0)
    }

    #[test]
    fn one_digit_is_square_more_digits_are_wider() {
        let one = marker(7, Rgb(255, 59, 48)).bounds();
        assert_eq!((one.width, one.height), (26.0, 26.0));
        let two = marker(12, Rgb(255, 59, 48)).bounds();
        assert!(two.width > 26.0 && two.height == 26.0, "{two:?}");
        assert_eq!(two.x + two.width / 2.0, 50.0, "stays centred");
    }

    #[test]
    fn text_colour_depends_on_the_background() {
        assert!(is_light(Rgb(0xFF, 0xD6, 0x0A)), "yellow gets dark text");
        assert!(!is_light(Rgb(0xFF, 0x9F, 0x0A)), "orange gets white text");
        assert!(!is_light(Rgb(0x0A, 0x84, 0xFF)));
    }

    #[test]
    fn hit_test_uses_the_marker_with_tolerance() {
        let m = marker(1, Rgb(255, 59, 48));
        assert!(m.hit_test(Point::new(50.0, 40.0), 0.0));
        assert!(m.hit_test(Point::new(64.0, 40.0), 4.0));
        assert!(!m.hit_test(Point::new(70.0, 40.0), 4.0));
    }

    #[test]
    fn renders_box_and_white_digit() {
        let mut pixels = vec![0u8; 100 * 80 * 4];
        let mut pixmap = PixmapMut::from_bytes(&mut pixels, 100, 80).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(128, 128, 128, 255));
        marker(1, Rgb(0x0A, 0x84, 0xFF)).render(&mut pixmap, Transform::identity());

        let at = |x: usize, y: usize| -> [u8; 3] {
            pixels[(y * 100 + x) * 4..][..3].try_into().unwrap()
        };
        assert_eq!(
            at(41, 31),
            [0xFF, 0x84, 0x0A],
            "blue box (BGR) near a corner"
        );
        // Somewhere in the middle column there is white from the digit "1".
        let white = (30..50).any(|y| at(50, y).iter().all(|&c| c > 230));
        assert!(white, "the digit is drawn");
        assert_eq!(at(5, 5), [128, 128, 128], "outside is untouched");
    }
}
