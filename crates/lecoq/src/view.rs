//! Where the Image sits on the canvas: the zoom (§3.1) and, when zoomed, its position.
//!
//! Pure geometry, in points. Scales are image pixels per physical pixel, so 100% is one image pixel
//! per screen pixel at any HiDPI scale factor.

use lecoq_codec::Size;

/// The factor of one `+` / `-` press. Ctrl+wheel uses egui's smooth zoom delta instead.
pub const ZOOM_STEP: f32 = 1.25;
const MIN_SCALE: f32 = 0.01;
const MAX_SCALE: f32 = 32.0;

/// How the Image is scaled on the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// Fit to the canvas, never above 100% (§3.1 `0`, the default for every new Image).
    Fit,
    /// Image pixels per physical pixel (1.0 = 100%).
    Scale(f32),
}

impl Zoom {
    /// What the status bar shows: `Fit` or a percentage (§2).
    pub fn label(self) -> String {
        match self {
            Zoom::Fit => crate::strings::ZOOM_FIT.to_owned(),
            Zoom::Scale(scale) => crate::strings::zoom_percent(scale),
        }
    }
}

/// The zoom and, when not fit, where the Image's centre sits relative to the canvas centre.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub zoom: Zoom,
    /// Image centre minus canvas centre, in points. Always zero when fit.
    pan: egui::Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self::fit()
    }
}

impl View {
    pub fn fit() -> Self {
        Self {
            zoom: Zoom::Fit,
            pan: egui::Vec2::ZERO,
        }
    }

    /// The scale actually used for an Image of `original` pixels on a canvas of `area` points.
    pub fn scale(&self, original: Size, area: egui::Rect, ppp: f32) -> f32 {
        match self.zoom {
            Zoom::Fit => fit_scale(original, area.size(), ppp),
            Zoom::Scale(scale) => scale,
        }
    }

    /// Where the Image is drawn, in points. Snapped to the physical pixel grid, so 100% is sharp.
    ///
    /// The position is kept in bounds: on an axis where the Image is smaller than the canvas it is
    /// centred, otherwise it covers the canvas edge to edge.
    pub fn image_rect(&mut self, original: Size, area: egui::Rect, ppp: f32) -> egui::Rect {
        let size = size_points(original, self.scale(original, area, ppp), ppp);
        self.pan = clamp_pan(self.pan, size, area.size());
        let rect = egui::Rect::from_center_size(area.center() + self.pan, size);
        let min = (rect.min.to_vec2() * ppp).round() / ppp;
        egui::Rect::from_min_size(min.to_pos2(), size)
    }

    /// Move the Image by `delta` points, kept in bounds. Does nothing at Fit.
    pub fn pan_by(&mut self, delta: egui::Vec2, original: Size, area: egui::Rect, ppp: f32) {
        let Zoom::Scale(scale) = self.zoom else {
            return;
        };
        let size = size_points(original, scale, ppp);
        self.pan = clamp_pan(self.pan + delta, size, area.size());
    }

    /// Multiply the scale by `factor`, keeping the Image point under `anchor` (default: the canvas
    /// centre) where it is.
    pub fn zoom_by(
        &mut self,
        factor: f32,
        anchor: Option<egui::Pos2>,
        original: Size,
        area: egui::Rect,
        ppp: f32,
    ) {
        let old_scale = self.scale(original, area, ppp);
        let new_scale = (old_scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let k = new_scale / old_scale;
        let anchor = anchor.unwrap_or(area.center());
        let old_center = area.center() + self.pan;
        let new_center = anchor + (old_center - anchor) * k;
        self.zoom = Zoom::Scale(new_scale);
        self.pan = new_center - area.center();
        let size = size_points(original, new_scale, ppp);
        self.pan = clamp_pan(self.pan, size, area.size());
    }
}

/// The scale that fits `original` pixels into `available` points, never above 100%.
fn fit_scale(original: Size, available: egui::Vec2, ppp: f32) -> f32 {
    let available_px = available * ppp;
    (available_px.x / original.width as f32)
        .min(available_px.y / original.height as f32)
        .min(1.0)
}

fn size_points(original: Size, scale: f32, ppp: f32) -> egui::Vec2 {
    egui::vec2(original.width as f32, original.height as f32) * scale / ppp
}

fn clamp_pan(pan: egui::Vec2, size: egui::Vec2, area: egui::Vec2) -> egui::Vec2 {
    let axis = |pan: f32, size: f32, area: f32| {
        let slack = ((size - area) / 2.0).max(0.0);
        pan.clamp(-slack, slack)
    };
    egui::vec2(axis(pan.x, size.x, area.x), axis(pan.y, size.y, area.y))
}
