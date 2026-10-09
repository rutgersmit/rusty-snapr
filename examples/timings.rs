//! Rough timings of the heavy steps on large screens, to compare with the C# version.
//! Run with `cargo run --release --example timings`.

use std::time::{Duration, Instant};

use snapr_core::capture;
use snapr_core::editor::annotation::{Annotation, Shape};
use snapr_core::editor::arrow::Arrow;
use snapr_core::editor::number::Number;
use snapr_core::editor::pixelate::Pixelate;
use snapr_core::editor::{Bitmap, PALETTE, png, render};
use snapr_core::geometry::Point;

fn time<T>(label: &str, mut f: impl FnMut() -> T) {
    // Best of a few runs, to keep noise down.
    let best = (0..5)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(f());
            start.elapsed()
        })
        .min()
        .unwrap_or(Duration::ZERO);
    println!("  {label:<44} {:>7.1} ms", best.as_secs_f64() * 1000.0);
}

fn screenshot(width: u32, height: u32) -> Bitmap {
    let mut image = Bitmap::new(width, height);
    for (i, p) in image.pixels_mut().chunks_exact_mut(4).enumerate() {
        p[0] = (i % 251) as u8;
        p[1] = (i / 7 % 241) as u8;
        p[2] = (i / 13 % 239) as u8;
    }
    image
}

fn main() {
    for (name, width, height) in [
        ("4K (3840x2160)", 3840, 2160),
        ("two 4K screens (7680x2160)", 7680, 2160),
    ] {
        println!("{name}");
        let image = screenshot(width, height);
        let mut dimmed = vec![0u8; image.pixels().len()];
        time("overlay: darken the whole desktop", || {
            capture::dim_pixels(image.pixels(), &mut dimmed)
        });

        let mut arrow = Arrow::new(Point::new(200.0, 200.0), PALETTE[0], 7.0);
        arrow.end = Point::new(1500.0, 900.0);
        let mut pixelate = Pixelate::new(Point::new(100.0, 1200.0), 9, 1);
        pixelate.update(Point::new(1900.0, 1500.0), &image);
        let annotations: Vec<Annotation> = [
            Shape::Arrow(arrow),
            Shape::Number(Number::new(Point::new(300.0, 300.0), 1, PALETTE[4], 26.0)),
            Shape::Pixelate(pixelate),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, shape)| Annotation {
            id: i as u32,
            shape,
        })
        .collect();

        time("editor: pixelate a 1800x300 region", || {
            let mut p = Pixelate::new(Point::new(100.0, 1200.0), 9, 2);
            p.update(Point::new(1900.0, 1500.0), &image);
            p
        });
        time("copy or save: flatten at full size", || {
            render::flatten(&image, &annotations)
        });
        // The editor shows big captures scaled down to the window and draws at that size.
        let window = Bitmap::new(1766, 950);
        time("editor: one drawing frame (1766x950 window)", || {
            let mut frame = window.clone();
            render::draw_annotations_scaled(&mut frame, &annotations[..1], 1766.0 / width as f32);
            frame
        });
        time("clipboard/save: PNG encode", || png::encode(&image));
    }
}
