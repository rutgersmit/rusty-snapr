//! PNG encoding of a [`Bitmap`], for saving and for the "PNG" clipboard format.

use super::Bitmap;

/// Encodes an opaque BGRA bitmap as an RGB PNG (no alpha channel: screenshots are opaque, and it
/// keeps the file smaller).
pub fn encode(image: &Bitmap) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(image.width() as usize * image.height() as usize * 3);
    for p in image.pixels().chunks_exact(4) {
        rgb.extend_from_slice(&[p[2], p[1], p[0]]);
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    // Writing to a Vec cannot fail for a well-formed image.
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(&rgb).expect("PNG data");
    writer.finish().expect("PNG end");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_a_decoder() {
        let mut image = Bitmap::new(3, 2);
        image.pixels_mut()[..4].copy_from_slice(&[10, 20, 30, 255]); // B, G, R, A

        let bytes = encode(&image);
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buffer).unwrap();

        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(&buffer[..3], &[30, 20, 10]); // R, G, B
        assert_eq!(&buffer[3..6], &[0, 0, 0]);
    }
}
