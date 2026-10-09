//! Flattening the screenshot and its annotations into one image.

use tiny_skia::{PixmapMut, Transform};

use super::Bitmap;
use super::annotation::Annotation;

/// The screenshot with all annotations drawn on it, at the original pixel size.
pub fn flatten<'a>(
    image: &Bitmap,
    annotations: impl IntoIterator<Item = &'a Annotation>,
) -> Bitmap {
    let mut out = image.clone();
    draw_annotations(&mut out, annotations);
    out
}

/// Draws annotations onto a bitmap (in image coordinates).
pub fn draw_annotations<'a>(
    target: &mut Bitmap,
    annotations: impl IntoIterator<Item = &'a Annotation>,
) {
    let (width, height) = (target.width(), target.height());
    let Some(mut pixmap) = PixmapMut::from_bytes(target.pixels_mut(), width, height) else {
        return;
    };
    for annotation in annotations {
        annotation.render(&mut pixmap, Transform::identity());
    }
}
