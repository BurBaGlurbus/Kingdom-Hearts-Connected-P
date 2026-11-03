//! The bounds of a clip's contents.

use crate::geom::Rect;
use crate::player::{Kind, Sprite};

/// The untransformed bounds of what a clip holds (shapes and bitmaps, through its children).
pub fn local_bounds(s: &Sprite) -> Rect {
    let mut r = Rect::EMPTY;
    for node in s.children.iter().chain(&s.extra) {
        let b = match &node.kind {
            Kind::Shape(shape) => shape.bounds.rect(),
            Kind::Morph(shape, _) => shape.bounds.rect(),
            Kind::Image(image) => Rect::new(0.0, 0.0, image.size.0, image.size.1),
            Kind::Sprite(child) => local_bounds(child),
            _ => Rect::EMPTY,
        };
        if !b.is_empty() {
            r = r.union(&b.transform(&node.matrix));
        }
    }
    r
}
