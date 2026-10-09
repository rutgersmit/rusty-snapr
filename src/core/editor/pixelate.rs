//! A rectangle of the original screenshot replaced by coarse blocks, making text unreadable
//! (port of `PixelateAnnotation.cs`).

use std::sync::Arc;

use tiny_skia::{FilterQuality, PixmapMut, PixmapPaint, PixmapRef, Transform};

use super::{Bitmap, pixelator, round_like_dotnet};
use crate::geometry::{IRect, Point};

#[derive(Debug, Clone, PartialEq)]
pub struct Pixelate {
    start: Point,
    block_size: i32,
    /// Fresh noise pattern per region.
    seed: i32,
    region: IRect,
    /// The pixelated region, shared so undo/redo copies are cheap.
    pixels: Option<Arc<Bitmap>>,
}

impl Pixelate {
    pub fn new(start: Point, block_size: i32, seed: i32) -> Self {
        Self {
            start,
            block_size,
            seed,
            region: IRect::default(),
            pixels: None,
        }
    }

    pub fn region(&self) -> IRect {
        self.region
    }

    pub fn is_empty(&self) -> bool {
        self.region.width < 2 || self.region.height < 2
    }

    /// Stretches the region from the start point to `current` and re-pixelates `source`.
    pub fn update(&mut self, current: Point, source: &Bitmap) {
        let clamp = |v: f64, max: u32| round_like_dotnet(v).clamp(0.0, max as f64) as i32;
        let (w, h) = (source.width(), source.height());
        let x1 = clamp(self.start.x.min(current.x), w);
        let y1 = clamp(self.start.y.min(current.y), h);
        let x2 = clamp(self.start.x.max(current.x), w);
        let y2 = clamp(self.start.y.max(current.y), h);

        let region = IRect::new(x1, y1, x2 - x1, y2 - y1);
        if region == self.region {
            return;
        }
        self.region = region;
        self.pixels = if self.is_empty() {
            None
        } else {
            Some(Arc::new(pixelator::pixelate(
                source,
                region,
                self.block_size,
                self.seed,
            )))
        };
    }

    pub fn render(&self, pixmap: &mut PixmapMut<'_>, transform: Transform) {
        let Some(pixels) = &self.pixels else { return };
        let Some(image) = PixmapRef::from_bytes(pixels.pixels(), pixels.width(), pixels.height())
        else {
            return;
        };
        // Nearest neighbour keeps the blocks crisp.
        let paint = PixmapPaint {
            quality: FilterQuality::Nearest,
            ..PixmapPaint::default()
        };
        pixmap.draw_pixmap(self.region.x, self.region.y, image, &paint, transform, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkerboard(width: u32, height: u32) -> Bitmap {
        let mut image = Bitmap::new(width, height);
        let stride = image.stride();
        for y in 0..height as usize {
            for x in 0..width as usize {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                image.pixels_mut()[y * stride + x * 4..][..3].copy_from_slice(&[v, v, v]);
            }
        }
        image
    }

    #[test]
    fn region_follows_the_drag_in_any_direction() {
        let source = checkerboard(100, 80);
        let mut p = Pixelate::new(Point::new(60.0, 50.0), 8, 1);
        p.update(Point::new(20.4, 10.6), &source);
        assert_eq!(p.region(), IRect::new(20, 11, 40, 39));
        assert!(!p.is_empty());
        p.update(Point::new(500.0, -5.0), &source);
        assert_eq!(
            p.region(),
            IRect::new(60, 0, 40, 50),
            "clamped to the image"
        );
    }

    #[test]
    fn rounds_halves_to_even_like_dotnet() {
        let source = checkerboard(100, 80);
        let mut p = Pixelate::new(Point::new(10.5, 11.5), 8, 1);
        p.update(Point::new(30.0, 30.0), &source);
        assert_eq!((p.region().x, p.region().y), (10, 12));
    }

    #[test]
    fn tiny_regions_are_empty() {
        let source = checkerboard(100, 80);
        let mut p = Pixelate::new(Point::new(10.0, 10.0), 8, 1);
        p.update(Point::new(11.0, 40.0), &source);
        assert!(p.is_empty());
    }

    #[test]
    fn renders_blocks_over_the_region() {
        let source = checkerboard(64, 64);
        let mut p = Pixelate::new(Point::new(8.0, 8.0), 8, 3);
        p.update(Point::new(40.0, 40.0), &source);

        let mut target = source.clone();
        let (w, h) = (target.width(), target.height());
        let mut pixmap = PixmapMut::from_bytes(target.pixels_mut(), w, h).unwrap();
        p.render(&mut pixmap, Transform::identity());

        // Inside: one colour per 8×8 block (a checkerboard averages to grey ± noise).
        let block = target.pixel(8, 8);
        assert_eq!(target.pixel(15, 15), block);
        assert!((block[0] as i32 - 128).abs() <= 19, "{block:?}");
        // Outside: untouched.
        assert_eq!(target.pixel(0, 0), source.pixel(0, 0));
        assert_eq!(target.pixel(41, 41), source.pixel(41, 41));
    }
}
