//! What gets drawn: the Image and a stack of Annotations, bottom to top.
//!
//! A [`Scene`] can be cut at any Annotation ([`Scene::below`]): that is "everything below it in
//! z-order" (§4.2), which effect Annotations (highlight, obfuscate) read and can re-render
//! offscreen over any region (`effects::render_below`).

use lecoq_core::{Annotation, Document};
use skia_safe as sk;

#[derive(Clone, Copy)]
pub struct Scene<'a> {
    /// The whole Document (for what's derived from it, such as Step counter numbers).
    pub doc: &'a Document,
    /// The Image's pixels, drawn over `image_rect`; `None` for a scene without the Image.
    image: Option<&'a sk::Image>,
    /// The Image's bounds in Image pixels.
    pub image_rect: sk::Rect,
    /// The Annotations in this scene, bottom to top: a prefix of the Document's.
    annotations: &'a [Annotation],
}

impl<'a> Scene<'a> {
    pub fn new(image: &'a sk::Image, doc: &'a Document) -> Self {
        Self {
            doc,
            image: Some(image),
            image_rect: sk::Rect::from_wh(doc.width() as f32, doc.height() as f32),
            annotations: doc.annotations(),
        }
    }

    /// Nothing at all: no Image and no Annotations, only `doc` for what's derived from it. What
    /// a copied Annotation is drawn over in the clipboard PNG (§6.2).
    pub fn empty(doc: &'a Document) -> Self {
        Self {
            doc,
            image: None,
            image_rect: sk::Rect::from_wh(doc.width() as f32, doc.height() as f32),
            annotations: &[],
        }
    }

    /// The Image and the Annotations under the `index`th one.
    pub fn below(&self, index: usize) -> Self {
        Self {
            annotations: &self.annotations[..index],
            ..*self
        }
    }

    /// Draw the Image (if any), then each Annotation over what's below it. The canvas is in Image pixels.
    pub fn draw(&self, canvas: &sk::Canvas) {
        if let Some(image) = self.image {
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                self.image_rect,
                sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::Linear),
                &sk::Paint::default(),
            );
        }
        for (i, annotation) in self.annotations.iter().enumerate() {
            crate::shapes::draw(canvas, annotation, &self.below(i));
        }
    }
}
