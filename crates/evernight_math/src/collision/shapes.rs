use crate::{Angle, Vec2};

#[derive(Debug, Clone)]
pub enum Shape2D {
    Circle(Circle),
    Rectangle(Rectangle),
    Triangle(Triangle),
    Ellipse(Ellipse),
    Capsule(Capsule),
    Polygon(Polygon),
    Line(Line),
    Ray(Ray),
}

#[derive(Debug, Clone)]
pub struct Circle {
    pub center: Vec2,
    pub radius: f32,
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    pub position: Vec2,
    pub size: Vec2,
    pub rotation: Angle,
}

#[derive(Debug, Clone)]
pub struct Triangle {
    pub a: Vec2,
    pub b: Vec2,
    pub c: Vec2,
}

#[derive(Debug, Clone)]
pub struct Ellipse {
    pub center: Vec2,
    pub major_axis_angle: f32,
    pub radii: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct Capsule {
    pub start: Vec2,
    pub end: Vec2,
    pub radius: f32,
}

#[derive(Debug, Clone)]
pub struct Polygon {
    pub vertices: Vec<Vec2>, // Must be convex & CCW
}

#[derive(Debug, Clone)]
pub struct Line {
    pub start: Vec2,
    pub end: Vec2,
}

#[derive(Debug, Clone)]
pub struct Ray {
    pub origin: Vec2,
    pub direction: Vec2,
}

/// Axis-aligned bounding box used for broad-phase collision culling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Aabb {
    #[inline]
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Aabb { min_x, min_y, max_x, max_y }
    }

    /// Returns `true` if this AABB overlaps `other` (inclusive boundary).
    #[inline]
    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min_x <= other.max_x
            && other.min_x <= self.max_x
            && self.min_y <= other.max_y
            && other.min_y <= self.max_y
    }
}

/// Computes the world-space AABB of a `Shape2D`.
///
/// The shape must already be in world space (post-transform).
/// For `Ray`, which is unbounded, returns an infinite AABB.
pub fn aabb_of(shape: &Shape2D) -> Aabb {
    match shape {
        Shape2D::Circle(c) => Aabb {
            min_x: c.center.x - c.radius,
            min_y: c.center.y - c.radius,
            max_x: c.center.x + c.radius,
            max_y: c.center.y + c.radius,
        },
        Shape2D::Rectangle(r) => {
            // `position` is the center of the rectangle.
            // For a rotated rectangle, the AABB half-extents are:
            //   extent_x = half_w * |cos θ| + half_h * |sin θ|
            //   extent_y = half_w * |sin θ| + half_h * |cos θ|
            let half_w = r.size.x * 0.5;
            let half_h = r.size.y * 0.5;
            let cos_a = r.rotation.0.cos().abs();
            let sin_a = r.rotation.0.sin().abs();
            let extent_x = half_w * cos_a + half_h * sin_a;
            let extent_y = half_w * sin_a + half_h * cos_a;
            Aabb {
                min_x: r.position.x - extent_x,
                min_y: r.position.y - extent_y,
                max_x: r.position.x + extent_x,
                max_y: r.position.y + extent_y,
            }
        }
        Shape2D::Triangle(t) => Aabb {
            min_x: t.a.x.min(t.b.x).min(t.c.x),
            min_y: t.a.y.min(t.b.y).min(t.c.y),
            max_x: t.a.x.max(t.b.x).max(t.c.x),
            max_y: t.a.y.max(t.b.y).max(t.c.y),
        },
        Shape2D::Ellipse(e) => {
            // Tight AABB of a rotated ellipse:
            //   extent_x = sqrt((a·cos θ)² + (b·sin θ)²)
            //   extent_y = sqrt((a·sin θ)² + (b·cos θ)²)
            let (a, b) = e.radii;
            let cos_t = e.major_axis_angle.cos();
            let sin_t = e.major_axis_angle.sin();
            let extent_x = ((a * cos_t) * (a * cos_t) + (b * sin_t) * (b * sin_t)).sqrt();
            let extent_y = ((a * sin_t) * (a * sin_t) + (b * cos_t) * (b * cos_t)).sqrt();
            Aabb {
                min_x: e.center.x - extent_x,
                min_y: e.center.y - extent_y,
                max_x: e.center.x + extent_x,
                max_y: e.center.y + extent_y,
            }
        }
        Shape2D::Capsule(c) => Aabb {
            min_x: c.start.x.min(c.end.x) - c.radius,
            min_y: c.start.y.min(c.end.y) - c.radius,
            max_x: c.start.x.max(c.end.x) + c.radius,
            max_y: c.start.y.max(c.end.y) + c.radius,
        },
        Shape2D::Polygon(p) => {
            if p.vertices.is_empty() {
                return Aabb { min_x: 0.0, min_y: 0.0, max_x: 0.0, max_y: 0.0 };
            }
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for v in &p.vertices {
                if v.x < min_x { min_x = v.x; }
                if v.y < min_y { min_y = v.y; }
                if v.x > max_x { max_x = v.x; }
                if v.y > max_y { max_y = v.y; }
            }
            Aabb { min_x, min_y, max_x, max_y }
        }
        Shape2D::Line(l) => Aabb {
            min_x: l.start.x.min(l.end.x),
            min_y: l.start.y.min(l.end.y),
            max_x: l.start.x.max(l.end.x),
            max_y: l.start.y.max(l.end.y),
        },
        Shape2D::Ray(_) => {
            // Rays are unbounded; return infinite AABB.
            // The spatial hash grid will skip infinite AABBs on insert.
            Aabb {
                min_x: f32::NEG_INFINITY,
                min_y: f32::NEG_INFINITY,
                max_x: f32::INFINITY,
                max_y: f32::INFINITY,
            }
        }
    }
}
