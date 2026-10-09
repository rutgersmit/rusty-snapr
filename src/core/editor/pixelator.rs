//! Port of `Pixelator.cs`: replaces a region by coarse blocks of their average colour plus a
//! little noise, making text unreadable.
//!
//! Plain block averages of text can be partly reversed by matching them against rendered
//! candidate text (e.g. the Depix tool). The per-block noise makes that matching much harder,
//! though it is risk reduction rather than a guarantee. Blocks sit on a fixed grid over the whole
//! image and the noise depends only on the seed and the block's grid position, so the pattern
//! stays stable while a region is being dragged.

use super::Bitmap;
use crate::geometry::IRect;

/// How far (per channel, 0-255) each block's colour is randomly nudged.
pub const NOISE_AMPLITUDE: i32 = 18;

/// Returns `region` of `source` with every block of `block_size` pixels replaced by its average
/// colour plus a little noise. The region must be non-empty and lie inside the image.
pub fn pixelate(source: &Bitmap, region: IRect, block_size: i32, seed: i32) -> Bitmap {
    assert!(block_size > 0);
    assert!(
        !region.is_empty()
            && region.x >= 0
            && region.y >= 0
            && region.right() <= source.width() as i32
            && region.bottom() <= source.height() as i32,
        "pixelate region {region:?} outside {source:?}"
    );

    // Averages come from whole grid blocks (clipped to the image), not just the part inside the
    // region, so an edge block keeps its colour while the region's edge moves through it.
    let left = region.x - region.x % block_size;
    let top = region.y - region.y % block_size;
    let right = (source.width() as i32).min(ceil_to_block(region.right(), block_size));
    let bottom = (source.height() as i32).min(ceil_to_block(region.bottom(), block_size));

    let src_stride = source.stride();
    let src = source.pixels();

    let (width, height) = (region.width, region.height);
    let stride = width as usize * 4;
    let mut pixels = vec![0u8; stride * height as usize];

    for by in (top..bottom).step_by(block_size as usize) {
        let bh = block_size.min(bottom - by);
        for bx in (left..right).step_by(block_size as usize) {
            let bw = block_size.min(right - bx);

            let (mut b, mut g, mut r) = (0i64, 0i64, 0i64);
            for y in by..by + bh {
                let row = y as usize * src_stride;
                for x in bx..bx + bw {
                    let i = row + x as usize * 4;
                    b += src[i] as i64;
                    g += src[i + 1] as i64;
                    r += src[i + 2] as i64;
                }
            }

            let count = (bw * bh) as i64;
            let noise = noise(seed, bx / block_size, by / block_size) as i64;
            let color = [
                channel(b / count + noise),
                channel(g / count + noise),
                channel(r / count + noise),
                255,
            ];

            // Paint only the part of this block that lies inside the region.
            let x0 = (bx - region.x).max(0);
            let x1 = width.min(bx + bw - region.x);
            let y0 = (by - region.y).max(0);
            let y1 = height.min(by + bh - region.y);
            for y in y0..y1 {
                let row = y as usize * stride;
                for x in x0..x1 {
                    let i = row + x as usize * 4;
                    pixels[i..i + 4].copy_from_slice(&color);
                }
            }
        }
    }

    Bitmap::from_pixels(width as u32, height as u32, pixels)
}

/// Deterministic pseudo-random offset in `[-NOISE_AMPLITUDE, NOISE_AMPLITUDE]` for one block.
/// Bit-for-bit the same as the C# version (unsigned 32-bit arithmetic).
pub fn noise(seed: i32, column: i32, row: i32) -> i32 {
    let mut h = (seed as u32)
        ^ (column as u32).wrapping_mul(0x9E37_79B1)
        ^ (row as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h % (2 * NOISE_AMPLITUDE as u32 + 1)) as i32 - NOISE_AMPLITUDE
}

fn ceil_to_block(value: i32, block_size: i32) -> i32 {
    (value + block_size - 1) / block_size * block_size
}

fn channel(value: i64) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gradient so every block has a different average.
    fn gradient(width: u32, height: u32) -> Bitmap {
        let mut image = Bitmap::new(width, height);
        let stride = image.stride();
        for y in 0..height as usize {
            for x in 0..width as usize {
                let i = y * stride + x * 4;
                let p = &mut image.pixels_mut()[i..i + 4];
                p[0] = (x * 7 % 256) as u8;
                p[1] = (y * 5 % 256) as u8;
                p[2] = ((x + y) * 3 % 256) as u8;
            }
        }
        image
    }

    #[test]
    fn noise_matches_reference_values() {
        for (seed, column, row, expected) in REFERENCE_NOISE {
            assert_eq!(
                noise(seed, column, row),
                expected,
                "noise({seed}, {column}, {row})"
            );
        }
    }

    // (seed, column, row, expected), printed by the original C# Pixelator.Noise.
    const REFERENCE_NOISE: [(i32, i32, i32, i32); 8] = [
        (0, 0, 0, -18),
        (1, 0, 0, 14),
        (42, 3, 7, 2),
        (12345, 10, 20, -9),
        (-7, 5, 5, 6),
        (i32::MAX, 100, 200, 16),
        (i32::MIN, 1, 1, -15),
        (987654321, 0, 99, -18),
    ];

    #[test]
    fn output_matches_the_csharp_version() {
        // FNV-1a hash of the C# Pixelator.Pixelate output for the same gradient, region, block
        // size and seed (run with the original algorithm, WPF pixel copying replaced by arrays).
        let out = pixelate(&gradient(64, 48), IRect::new(5, 7, 41, 37), 8, 99);
        assert_eq!(out.pixels().len(), 6068);
        let hash = out.pixels().iter().fold(14695981039346656037u64, |h, &b| {
            (h ^ b as u64).wrapping_mul(1099511628211)
        });
        assert_eq!(hash, 15094846919501692435);
    }

    #[test]
    fn noise_stays_within_amplitude_and_varies() {
        let values: Vec<i32> = (0..500).map(|i| noise(42, i % 25, i / 25)).collect();
        assert!(
            values
                .iter()
                .all(|v| (-NOISE_AMPLITUDE..=NOISE_AMPLITUDE).contains(v))
        );
        let distinct: std::collections::HashSet<_> = values.iter().collect();
        assert!(distinct.len() > 30, "noise should use most of its range");
    }

    #[test]
    fn blocks_are_uniform_and_close_to_their_average() {
        let image = gradient(40, 30);
        let out = pixelate(&image, IRect::new(0, 0, 40, 30), 10, 7);
        assert_eq!((out.width(), out.height()), (40, 30));

        // Every pixel in a block has the same colour.
        for by in 0..3 {
            for bx in 0..4 {
                let first = out.pixel(bx * 10, by * 10);
                for y in by * 10..by * 10 + 10 {
                    for x in bx * 10..bx * 10 + 10 {
                        assert_eq!(out.pixel(x, y), first);
                    }
                }
                // Within noise distance of the true average.
                let avg = |c: usize| {
                    let mut sum = 0u32;
                    for y in by * 10..by * 10 + 10 {
                        for x in bx * 10..bx * 10 + 10 {
                            sum += image.pixel(x, y)[c] as u32;
                        }
                    }
                    (sum / 100) as i32
                };
                for (c, &value) in first.iter().take(3).enumerate() {
                    assert!((value as i32 - avg(c)).abs() <= NOISE_AMPLITUDE);
                }
                assert_eq!(first[3], 255);
            }
        }
    }

    #[test]
    fn pattern_is_stable_while_the_region_grows() {
        // Dragging a region larger must not change the blocks that were already covered,
        // including the partially covered edge blocks.
        let image = gradient(64, 64);
        let small = pixelate(&image, IRect::new(5, 7, 20, 13), 8, 99);
        let large = pixelate(&image, IRect::new(5, 7, 41, 37), 8, 99);
        for y in 0..small.height() {
            for x in 0..small.width() {
                assert_eq!(small.pixel(x, y), large.pixel(x, y), "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn edge_blocks_are_clipped_to_the_image() {
        let image = gradient(23, 17);
        let out = pixelate(&image, IRect::new(0, 0, 23, 17), 8, 1);
        assert_eq!((out.width(), out.height()), (23, 17));
        // The last, partial block column (x 16..23) is uniform.
        let first = out.pixel(16, 16);
        assert_eq!(out.pixel(22, 16), first);
    }

    #[test]
    fn different_seeds_give_different_noise() {
        let image = gradient(64, 64);
        let a = pixelate(&image, IRect::new(0, 0, 64, 64), 8, 1);
        let b = pixelate(&image, IRect::new(0, 0, 64, 64), 8, 2);
        assert_ne!(a, b);
    }
}
