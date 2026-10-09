//! Golden-image test: a fixed scene with every kind of annotation, compared with a reference PNG,
//! so changes to how annotations look are noticed.
//!
//! After an intended change, regenerate the reference with:
//!     SNAPR_UPDATE_GOLDEN=1 cargo test --test golden
//! and look at `tests/golden/scene.png` before committing it.

use std::path::Path;

use snapr_core::editor::annotation::{Annotation, Shape};
use snapr_core::editor::arrow::Arrow;
use snapr_core::editor::number::Number;
use snapr_core::editor::pixelate::Pixelate;
use snapr_core::editor::{Bitmap, PALETTE, png, render};
use snapr_core::geometry::Point;

/// Allowed difference per channel (anti-aliasing may differ by a hair between CPUs).
const TOLERANCE: u8 = 2;

fn background() -> Bitmap {
    let (width, height) = (240u32, 140u32);
    let mut image = Bitmap::new(width, height);
    let stride = image.stride();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let i = y * stride + x * 4;
            // A gradient with some stripes, so pixelation has something to average.
            let stripe = if (x / 3 + y / 5) % 2 == 0 { 40 } else { 0 };
            image.pixels_mut()[i..i + 3].copy_from_slice(&[
                (200 - y) as u8,
                (60 + x / 2) as u8,
                (90 + stripe + y / 2) as u8,
            ]);
        }
    }
    image
}

fn scene() -> Bitmap {
    let image = background();
    let mut pixelate = Pixelate::new(Point::new(130.0, 80.0), 9, 12345);
    pixelate.update(Point::new(225.0, 128.0), &image);
    let mut arrow = Arrow::new(Point::new(20.0, 20.0), PALETTE[0], 7.0);
    arrow.end = Point::new(150.0, 70.0);
    let shapes = [
        Shape::Pixelate(pixelate),
        Shape::Arrow(arrow),
        Shape::Number(Number::new(Point::new(40.0, 100.0), 1, PALETTE[4], 26.0)),
        Shape::Number(Number::new(Point::new(90.0, 100.0), 12, PALETTE[2], 26.0)),
    ];
    let annotations: Vec<Annotation> = shapes
        .into_iter()
        .enumerate()
        .map(|(i, shape)| Annotation {
            id: i as u32 + 1,
            shape,
        })
        .collect();
    render::flatten(&image, &annotations)
}

fn decode(path: &Path) -> Bitmap {
    let file =
        std::fs::File::open(path).expect("reference image (regenerate with SNAPR_UPDATE_GOLDEN=1)");
    let mut reader = ::png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let mut rgb = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgb).unwrap();
    assert_eq!(info.color_type, ::png::ColorType::Rgb);
    let mut bgra = Vec::with_capacity(rgb.len() / 3 * 4);
    for p in rgb.chunks_exact(3) {
        bgra.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    Bitmap::from_pixels(info.width, info.height, bgra)
}

#[test]
fn annotations_look_as_before() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/scene.png");
    let actual = scene();
    if std::env::var_os("SNAPR_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, png::encode(&actual)).unwrap();
        return;
    }

    let expected = decode(&path);
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height())
    );
    let mut worst = 0u8;
    let mut differing = 0usize;
    for (a, e) in actual
        .pixels()
        .chunks_exact(4)
        .zip(expected.pixels().chunks_exact(4))
    {
        let diff = (0..3).map(|c| a[c].abs_diff(e[c])).max().unwrap();
        worst = worst.max(diff);
        if diff > TOLERANCE {
            differing += 1;
        }
    }
    if differing > 0 {
        let out = std::env::temp_dir().join("snapr-golden-actual.png");
        std::fs::write(&out, png::encode(&actual)).unwrap();
        panic!(
            "{differing} pixels differ (worst {worst}); actual image written to {}",
            out.display()
        );
    }
}
