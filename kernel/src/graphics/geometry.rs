//! 2D Geometry Primitives and Rectangular Clipping.
//!
//! Provides integer points, bounding rectangles, collision detection,
//! clipping intersections, and coordinate transformation utilities.

/// Represents an integer 2D screen coordinate point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    /// Creates a new Point.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Represents an integer 2D bounding rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[allow(dead_code)]
impl Rect {
    /// Creates a new bounding rectangle.
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Returns the rightmost x-coordinate (exclusive).
    #[inline(always)]
    pub const fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    /// Returns the bottommost y-coordinate (exclusive).
    #[inline(always)]
    pub const fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    /// Returns true if the rectangle has zero area.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns true if the point falls within the bounds of this rectangle.
    #[inline]
    pub fn contains(&self, pt: Point) -> bool {
        pt.x >= self.x && pt.x < self.right() && pt.y >= self.y && pt.y < self.bottom()
    }

    /// Returns true if this rectangle overlaps with another rectangle.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Computes the intersection of this rectangle and another rectangle.
    ///
    /// Returns `Some(Rect)` if they overlap, or `None` if they are disjoint.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());

        if x2 > x1 && y2 > y1 {
            Some(Rect {
                x: x1,
                y: y1,
                width: (x2 - x1) as u32,
                height: (y2 - y1) as u32,
            })
        } else {
            None
        }
    }

    /// Insets the rectangle by `amount` pixels on all sides.
    pub fn inset(&self, amount: i32) -> Rect {
        let double_amount = (amount * 2).max(0) as u32;
        if self.width <= double_amount || self.height <= double_amount {
            Rect::new(self.x, self.y, 0, 0)
        } else {
            Rect::new(
                self.x + amount,
                self.y + amount,
                self.width - double_amount,
                self.height - double_amount,
            )
        }
    }

    /// Translates the rectangle by `(dx, dy)`.
    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}
