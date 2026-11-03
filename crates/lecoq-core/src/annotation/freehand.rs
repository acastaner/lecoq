//! Freehand (§4.1 F): a stroke through the pointer's path, lightly smoothed.

use super::Shape;
use super::line::distance_to_segment;
use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Freehand {
    /// The stroke's points, in drawing order. Never empty.
    pub points: Vec<Point>,
}

impl Freehand {
    /// §4.3: line colour, thickness, shadow.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::LineColour,
        PropertyKind::Thickness,
        PropertyKind::Shadow,
    ];

    pub fn new(points: Vec<Point>) -> Self {
        assert!(
            !points.is_empty(),
            "a freehand stroke has at least one point"
        );
        Self { points }
    }

    /// The pointer's path, lightly smoothed: points closer than `min_step` to the previous kept
    /// one are dropped (pointer jitter), then two passes of a [1, 2, 1] / 4 moving average pull
    /// each interior point towards its neighbours. The ends stay where the stroke started and
    /// stopped.
    pub fn smoothed(path: &[Point], min_step: f32) -> Self {
        let mut points: Vec<Point> = Vec::with_capacity(path.len());
        for (i, &p) in path.iter().enumerate() {
            let last = i + 1 == path.len();
            match points.last() {
                Some(&prev) if !last && prev.distance(p) < min_step => {}
                Some(&prev) if last && prev.distance(p) < min_step && points.len() > 1 => {
                    // Keep the true end point instead of the last kept one.
                    *points.last_mut().expect("not empty") = p;
                }
                _ => points.push(p),
            }
        }
        for _ in 0..2 {
            if points.len() < 3 {
                break;
            }
            let mut next = points.clone();
            for i in 1..points.len() - 1 {
                let (a, b, c) = (points[i - 1], points[i], points[i + 1]);
                next[i] = Point::new((a.x + 2.0 * b.x + c.x) / 4.0, (a.y + 2.0 * b.y + c.y) / 4.0);
            }
            points = next;
        }
        Self::new(points)
    }
}

impl Shape for Freehand {
    fn bounds(&self) -> Rect {
        let first = Rect::from_corners(self.points[0], self.points[0]);
        self.points
            .iter()
            .fold(first, |r, &p| r.union(&Rect::from_corners(p, p)))
    }

    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        let reach = style.thickness / 2.0 + tolerance;
        if self.points.len() == 1 {
            return p.distance(self.points[0]) <= reach;
        }
        self.points
            .windows(2)
            .any(|w| distance_to_segment(p, w[0], w[1]) <= reach)
    }

    fn transform(&mut self, t: &Affine) {
        for p in &mut self.points {
            *p = t.apply(*p);
        }
    }

    fn image_transform(&mut self, t: &ImageTransform) {
        self.transform(&t.affine());
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}
