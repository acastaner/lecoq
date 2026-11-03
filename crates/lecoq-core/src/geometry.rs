//! Geometry in Image pixel coordinates: the origin is the Image's top-left corner, x grows right
//! and y grows down, one unit is one full-resolution Image pixel. Annotations are stored in these
//! coordinates, whatever the zoom or the size of the display decode.

use std::ops::{Add, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point) -> f32 {
        (self - other).length()
    }
}

/// A displacement between two points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

impl Vector {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }
}

impl Sub for Point {
    type Output = Vector;
    fn sub(self, rhs: Point) -> Vector {
        Vector::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Add<Vector> for Point {
    type Output = Point;
    fn add(self, rhs: Vector) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

/// An axis-aligned rectangle. Always normalised: `min` is the top-left corner, `max` the
/// bottom-right one.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    /// The rectangle spanned by two opposite corners, in any order.
    pub fn from_corners(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn from_min_size(min: Point, width: f32, height: f32) -> Self {
        Self::from_corners(min, Point::new(min.x + width, min.y + height))
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Point {
        Point::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// Grown by `by` on every side (shrunk when negative).
    pub fn expand(&self, by: f32) -> Self {
        Self::from_corners(
            Point::new(self.min.x - by, self.min.y - by),
            Point::new(self.max.x + by, self.max.y + by),
        )
    }

    /// The smallest rectangle containing both.
    pub fn union(&self, other: &Rect) -> Self {
        Self {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// The overlap of two rectangles, `None` when they don't overlap (touching edges don't).
    pub fn intersect(&self, other: &Rect) -> Option<Self> {
        let min = Point::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y));
        let max = Point::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y));
        (min.x < max.x && min.y < max.y).then_some(Self { min, max })
    }

    /// Where a handle sits on this rectangle.
    pub fn handle_position(&self, handle: Handle) -> Point {
        let (u, v) = handle.anchor();
        Point::new(
            self.min.x + u * self.width(),
            self.min.y + v * self.height(),
        )
    }

    /// The rectangle after dragging `handle` to `to`: the edges the handle controls follow the
    /// pointer, the others stay put. Dragging past the opposite edge flips the rectangle.
    pub fn with_handle_at(&self, handle: Handle, to: Point) -> Self {
        let (u, v) = handle.anchor();
        let mut a = self.min;
        let mut b = self.max;
        // Each axis: anchor 0 moves the min edge, 1 the max edge, 0.5 neither.
        if u == 0.0 {
            a.x = to.x;
        } else if u == 1.0 {
            b.x = to.x;
        }
        if v == 0.0 {
            a.y = to.y;
        } else if v == 1.0 {
            b.y = to.y;
        }
        Self::from_corners(a, b)
    }
}

/// The 8 resize handles of a selection box (§2), clockwise from the top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    /// The handle's position as a fraction of the box: (0, 0) top-left, (1, 1) bottom-right.
    pub fn anchor(self) -> (f32, f32) {
        match self {
            Handle::TopLeft => (0.0, 0.0),
            Handle::Top => (0.5, 0.0),
            Handle::TopRight => (1.0, 0.0),
            Handle::Right => (1.0, 0.5),
            Handle::BottomRight => (1.0, 1.0),
            Handle::Bottom => (0.5, 1.0),
            Handle::BottomLeft => (0.0, 1.0),
            Handle::Left => (0.0, 0.5),
        }
    }
}

/// A 2D affine transform: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
///
/// Every Annotation type implements [`crate::Shape::transform`] with it: moving and resizing use
/// translations and rect-to-rect maps; Image operations (ADR 0001: crop, 90° rotations, flips,
/// resize) are affine too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(by: Vector) -> Self {
        Self {
            e: by.x,
            f: by.y,
            ..Self::IDENTITY
        }
    }

    /// Maps `from` onto `to` (scale and translate). A degenerate `from` axis maps to `to`'s min.
    pub fn rect_to_rect(from: Rect, to: Rect) -> Self {
        let sx = if from.width() > 0.0 {
            to.width() / from.width()
        } else {
            0.0
        };
        let sy = if from.height() > 0.0 {
            to.height() / from.height()
        } else {
            0.0
        };
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: to.min.x - from.min.x * sx,
            f: to.min.y - from.min.y * sy,
        }
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// An axis-aligned rectangle mapped through this transform (exact for scales, translations,
    /// 90° rotations and flips).
    pub fn apply_rect(&self, r: Rect) -> Rect {
        Rect::from_corners(self.apply(r.min), self.apply(r.max))
    }
}
