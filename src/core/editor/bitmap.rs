use crate::geometry::IRect;

/// An opaque image with 4 bytes per pixel, rows top to bottom, no padding.
///
/// The byte order is BGRA as Windows captures and displays it. Nothing in the core depends on
/// which channel is which, except code that converts to other formats (such as PNG).
#[derive(Clone, PartialEq, Eq)]
pub struct Bitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Bitmap {
    pub const BYTES_PER_PIXEL: usize = 4;

    /// A black, opaque image.
    pub fn new(width: u32, height: u32) -> Self {
        let mut pixels = vec![0; width as usize * height as usize * Self::BYTES_PER_PIXEL];
        pixels.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Wraps existing pixel data. Panics if the length does not match the size.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * Self::BYTES_PER_PIXEL
        );
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    pub fn stride(&self) -> usize {
        self.width as usize * Self::BYTES_PER_PIXEL
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = y as usize * self.stride() + x as usize * Self::BYTES_PER_PIXEL;
        self.pixels[i..i + 4].try_into().unwrap()
    }

    /// Copies a region into a new, standalone bitmap (so the full desktop capture can be dropped).
    /// The region must lie inside the image.
    pub fn crop(&self, region: IRect) -> Bitmap {
        assert!(
            !region.is_empty()
                && region.x >= 0
                && region.y >= 0
                && region.right() <= self.width as i32
                && region.bottom() <= self.height as i32,
            "crop region {region:?} outside {}x{} image",
            self.width,
            self.height
        );
        let row_bytes = region.width as usize * Self::BYTES_PER_PIXEL;
        let mut pixels = Vec::with_capacity(row_bytes * region.height as usize);
        for y in region.y..region.bottom() {
            let start = y as usize * self.stride() + region.x as usize * Self::BYTES_PER_PIXEL;
            pixels.extend_from_slice(&self.pixels[start..start + row_bytes]);
        }
        Bitmap::from_pixels(region.width as u32, region.height as u32, pixels)
    }
}

impl std::fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bitmap({}x{})", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_copies_the_region() {
        let mut image = Bitmap::new(4, 3);
        for (i, p) in image.pixels_mut().chunks_exact_mut(4).enumerate() {
            p[0] = i as u8;
        }
        let cropped = image.crop(IRect::new(1, 1, 2, 2));
        assert_eq!((cropped.width(), cropped.height()), (2, 2));
        assert_eq!(cropped.pixel(0, 0)[0], 5);
        assert_eq!(cropped.pixel(1, 1)[0], 10);
    }

    #[test]
    #[should_panic]
    fn crop_outside_panics() {
        Bitmap::new(4, 4).crop(IRect::new(2, 2, 3, 3));
    }
}
