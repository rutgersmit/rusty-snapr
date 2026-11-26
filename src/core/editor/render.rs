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

/// Draws annotations onto a bitmap of the image's size.
pub fn draw_annotations<'a>(
    target: &mut Bitmap,
    annotations: impl IntoIterator<Item = &'a Annotation>,
) {
    draw_annotations_scaled(target, annotations, 1.0);
}

/// Draws annotations onto a bitmap that shows the image at `zoom` times its size.
pub fn draw_annotations_scaled<'a>(
    target: &mut Bitmap,
    annotations: impl IntoIterator<Item = &'a Annotation>,
    zoom: f32,
) {
    draw_annotations_transformed(target, annotations, Transform::from_scale(zoom, zoom));
}

/// Draws annotations onto a bitmap, mapping image pixels with `transform` (a uniform scale and a
/// translation, such as for a cropped view).
pub fn draw_annotations_transformed<'a>(
    target: &mut Bitmap,
    annotations: impl IntoIterator<Item = &'a Annotation>,
    transform: Transform,
) {
    let (width, height) = (target.width(), target.height());
    let Some(mut pixmap) = PixmapMut::from_bytes(target.pixels_mut(), width, height) else {
        return;
    };
    for annotation in annotations {
        annotation.render(&mut pixmap, transform);
    }
}
