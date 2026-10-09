//! Editor logic shared by the on-screen editor and the exported image.

pub mod annotation;
pub mod arrow;
pub mod bitmap;
pub mod chrome;
pub mod history;
pub mod number;
pub mod pixelate;
pub mod pixelator;
pub mod png;
pub mod render;
pub mod state;

pub use bitmap::Bitmap;

/// An opaque colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The five annotation colours, in toolbar order (keys 1–5).
pub const PALETTE: [Rgb; 5] = [
    Rgb(0xFF, 0x3B, 0x30), // red
    Rgb(0xFF, 0x9F, 0x0A), // orange
    Rgb(0xFF, 0xD6, 0x0A), // yellow
    Rgb(0x30, 0xD1, 0x58), // green
    Rgb(0x0A, 0x84, 0xFF), // blue
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Arrow,
    Pixelate,
    Number,
}

/// Rounds like .NET's `Math.Round` (halves to even), so sizes match the C# version exactly.
pub fn round_like_dotnet(value: f64) -> f64 {
    value.round_ties_even()
}
