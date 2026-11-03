//! The canvas (§2): the Image, fit or zoomed (§3.1), with the editor's composite and selection box
//! on top. The editor gets the pointer through the closure passed to [`show`].

use lecoq_codec::Size;
use lecoq_core::{Handle, Point, Rect};

use crate::app::ShownImage;
use crate::tools::select::{End, HANDLE_SIZE};
use crate::view::View;

/// The side of the handle under the pointer, grown from [`HANDLE_SIZE`] so the hovered handle reads
/// as "grab me" (#49).
const HANDLE_HOVER_SIZE: f32 = 12.0;

/// Where the canvas and the Image ended up this frame, in points, and the mapping between screen
/// points and Image pixels.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub area: egui::Rect,
    pub image: Option<egui::Rect>,
    /// The size of the texture the Image was drawn from.
    pub drawn: Option<Size>,
    pub pixels_per_point: f32,
    /// The Image's full-resolution size, when one is shown.
    pub image_size: Option<Size>,
}

/// What the editor wants painted over (or instead of) the Image this frame.
#[derive(Default)]
pub struct EditorPaint {
    /// The Skia composite of the visible region, and where it goes (§7.2).
    pub composite: Option<(egui::TextureId, egui::Rect)>,
    /// The selection's bounds, in points: drawn as a dashed box with 8 handles. `None` for a lone
    /// line/arrow, which uses `endpoints` instead (#50).
    pub selection: Option<egui::Rect>,
    /// A lone line/arrow's two endpoints `[from, to]`, in points: drawn as two handles with no box
    /// (#50). Mutually exclusive with `selection`.
    pub endpoints: Option<[egui::Pos2; 2]>,
    /// Painted last, on top of everything (the text caret, #28).
    pub overlay: Vec<egui::Shape>,
    /// The crop tool's region, in points: the rest of the Image is dimmed.
    pub crop: Option<egui::Rect>,
    /// The select tool's rubber band, in points: a dashed box without handles.
    pub rubber_band: Option<egui::Rect>,
    /// The selection or crop handle under the pointer: drawn with the hover variant (#49). Purely
    /// presentational — it changes no hit radius and nothing a press does.
    pub hovered_handle: Option<Handle>,
    /// The endpoint handle of a lone line/arrow under the pointer: drawn with the same hover variant
    /// (#50). Purely presentational, like `hovered_handle`.
    pub hovered_end: Option<End>,
}

impl Layout {
    /// The canvas size in physical pixels.
    pub fn area_px(&self) -> Size {
        to_px(self.area.size(), self.pixels_per_point)
    }

    /// The Image's on-screen size in physical pixels.
    pub fn image_px(&self) -> Option<Size> {
        Some(to_px(self.image?.size(), self.pixels_per_point))
    }

    /// Image pixels per screen point at the current zoom.
    pub fn image_per_point(&self) -> f32 {
        match (self.image, self.image_size) {
            (Some(rect), Some(size)) if rect.width() > 0.0 => size.width as f32 / rect.width(),
            _ => 1.0,
        }
    }

    /// The Image pixel under a screen position.
    pub fn image_point(&self, pos: egui::Pos2) -> Point {
        let Some(rect) = self.image else {
            return Point::default();
        };
        let s = self.image_per_point();
        Point::new((pos.x - rect.min.x) * s, (pos.y - rect.min.y) * s)
    }

    /// Where an Image pixel is on screen.
    pub fn screen_point(&self, p: Point) -> egui::Pos2 {
        let Some(rect) = self.image else {
            return egui::Pos2::ZERO;
        };
        let s = self.image_per_point();
        egui::pos2(rect.min.x + p.x / s, rect.min.y + p.y / s)
    }

    pub fn rect_to_screen(&self, r: Rect) -> egui::Rect {
        egui::Rect::from_two_pos(self.screen_point(r.min), self.screen_point(r.max))
    }

    /// The part of the Image that is on screen: where it is (points), which Image pixels it
    /// shows, and its size in physical pixels. What the Skia composite renders (§7.2).
    pub fn visible(&self) -> Option<(egui::Rect, Rect, Size)> {
        let shown = self.image?.intersect(self.area);
        if shown.width() <= 0.0 || shown.height() <= 0.0 {
            return None;
        }
        let region = Rect::from_corners(self.image_point(shown.min), self.image_point(shown.max));
        let px = to_px(shown.size(), self.pixels_per_point);
        let px = Size::new(px.width.max(1), px.height.max(1));
        Some((shown, region, px))
    }
}

fn to_px(size: egui::Vec2, ppp: f32) -> Size {
    Size::new((size.x * ppp).round() as u32, (size.y * ppp).round() as u32)
}

/// Draw the canvas in the remaining space. Without an Image, `message` (an error or the
/// empty-window hint) is centred on it. `image_size` is the Image's current size (Image
/// operations change it, §4.7). `edit` handles the pointer (the canvas response) and says
/// what to paint on top of the Image. The canvas takes the keyboard focus only while text is
/// typed on it (§4.4); otherwise Tab passes it by and the keys stay the viewer's (§8.6).
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    shown: Option<&ShownImage>,
    image_size: Option<Size>,
    view: &mut View,
    message: Option<&str>,
    focusable: bool,
    edit: impl FnOnce(&mut egui::Ui, &Layout, &egui::Response) -> EditorPaint,
) -> Layout {
    let frame = egui::Frame::NONE.fill(ui.visuals().extreme_bg_color);
    egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let ppp = ui.ctx().pixels_per_point();
            let mut layout = Layout {
                area: rect,
                image: None,
                drawn: None,
                pixels_per_point: ppp,
                image_size,
            };
            let texture = shown.zip(image_size).map(|(image, size)| {
                let dest = view.image_rect(size, rect, ppp);
                layout.image = Some(dest);
                image.texture_for(to_px(dest.size(), ppp))
            });
            let sense = if focusable {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::CLICK | egui::Sense::DRAG
            };
            let response = ui
                .interact(rect, ui.scope_id().with("canvas"), sense)
                .accessible_name(crate::strings::CANVAS);
            let paint = edit(ui, &layout, &response);

            let painter = ui.painter().with_clip_rect(rect);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            if let (Some(texture), Some(dest)) = (texture, layout.image) {
                if let Some((composite, dest)) = paint.composite {
                    painter.image(composite, dest, uv, egui::Color32::WHITE);
                } else {
                    painter.image(texture.id(), dest, uv, egui::Color32::WHITE);
                }
                layout.drawn = Some(Size::new(
                    texture.size()[0] as u32,
                    texture.size()[1] as u32,
                ));
            } else if let Some(message) = message {
                ui.centered_and_justified(|ui| ui.label(message));
            }
            if let (Some(crop), Some(image)) = (paint.crop, layout.image) {
                paint_crop(&painter, image, crop, paint.hovered_handle);
            }
            if let Some(selection) = paint.selection {
                paint_selection(&painter, selection, paint.hovered_handle);
            }
            if let Some(endpoints) = paint.endpoints {
                paint_endpoints(&painter, endpoints, paint.hovered_end);
            }
            painter.extend(paint.overlay);
            if let Some(band) = paint.rubber_band {
                paint_dashed_box(&painter, band);
            }
            layout
        })
        .inner
}

/// The crop region (§4.1 C): the Image outside it dimmed, and a box with 8 handles. `hovered` is the
/// crop handle under the pointer, grown like the selection's (#49).
fn paint_crop(
    painter: &egui::Painter,
    image: egui::Rect,
    crop: egui::Rect,
    hovered: Option<Handle>,
) {
    let dim = egui::Color32::from_black_alpha(140);
    let strips = [
        egui::Rect::from_min_max(image.min, egui::pos2(image.max.x, crop.min.y)),
        egui::Rect::from_min_max(egui::pos2(image.min.x, crop.max.y), image.max),
        egui::Rect::from_min_max(
            egui::pos2(image.min.x, crop.min.y),
            egui::pos2(crop.min.x, crop.max.y),
        ),
        egui::Rect::from_min_max(
            egui::pos2(crop.max.x, crop.min.y),
            egui::pos2(image.max.x, crop.max.y),
        ),
    ];
    for strip in strips {
        if strip.is_positive() {
            painter.rect_filled(strip, 0.0, dim);
        }
    }
    paint_selection(painter, crop, hovered);
}

/// A dashed box, visible on light and dark Images. Returns its dark stroke.
fn paint_dashed_box(painter: &egui::Painter, r: egui::Rect) -> egui::Stroke {
    let dark = egui::Stroke::new(1.0, egui::Color32::from_rgb(20, 20, 20));
    let light = egui::Stroke::new(1.0, egui::Color32::WHITE);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.add(egui::Shape::line(corners.to_vec(), dark));
    painter.extend(egui::Shape::dashed_line(&corners, light, 5.0, 4.0));
    dark
}

/// The selection box (§2): dashed, with 8 handles. The handle under the pointer (`hovered`) is drawn
/// larger so the user can see which one a press will grab (#49); the box and the other handles are
/// drawn exactly as before.
fn paint_selection(painter: &egui::Painter, r: egui::Rect, hovered: Option<Handle>) {
    let dark = paint_dashed_box(painter, r);
    for handle in Handle::ALL {
        let (u, v) = handle.anchor();
        let center = egui::pos2(r.min.x + u * r.width(), r.min.y + v * r.height());
        paint_handle(painter, center, hovered == Some(handle), dark);
    }
}

/// A lone line/arrow's endpoint handles (#50): the same white squares `paint_selection` draws, one
/// at each end, with no dashed box. The hovered end grows like a box handle. `endpoints` is
/// `[from, to]`, in points.
fn paint_endpoints(painter: &egui::Painter, endpoints: [egui::Pos2; 2], hovered: Option<End>) {
    // The dark border that `paint_dashed_box` gives the box handles; endpoint handles share it so
    // they read identically, without a box to draw it from.
    let dark = egui::Stroke::new(1.0, egui::Color32::from_rgb(20, 20, 20));
    let [from, to] = endpoints;
    paint_handle(painter, from, hovered == Some(End::From), dark);
    paint_handle(painter, to, hovered == Some(End::To), dark);
}

/// One selection handle: a filled white square with the dark border, centred on `center`. Grown to
/// the hover size when `hovered` (#49) — growing it is the whole cue: the fill and the border are
/// unchanged, so the handle reads brighter (more white, proportionally less border) with no colour
/// shift. Shared by the bounding box's 8 handles and a line/arrow's 2 endpoint handles (#50).
fn paint_handle(painter: &egui::Painter, center: egui::Pos2, hovered: bool, border: egui::Stroke) {
    let side = if hovered {
        HANDLE_HOVER_SIZE
    } else {
        HANDLE_SIZE
    };
    let square = egui::Rect::from_center_size(center, egui::vec2(side, side));
    painter.rect(
        square,
        0.0,
        egui::Color32::WHITE,
        border,
        egui::StrokeKind::Inside,
    );
}
