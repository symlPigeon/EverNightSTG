use crate::{Angle, Vec2};

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

pub struct Circle {
    pub center: Vec2,
    pub radius: f32,
}

pub struct Rectangle {
    pub position: Vec2,
    pub size: Vec2,
    pub rotation: Angle,
}

pub struct Triangle {
    pub a: Vec2,
    pub b: Vec2,
    pub c: Vec2,
}

pub struct Ellipse {
    pub center: Vec2,
    pub major_axis_angle: f32,
    pub radii: (f32, f32),
}

pub struct Capsule {
    pub start: Vec2,
    pub end: Vec2,
    pub radius: f32,
}

pub struct Polygon {
    pub vertices: Vec<Vec2>, // Must be convex & CCW
}

pub struct Line {
    pub start: Vec2,
    pub end: Vec2,
}

pub struct Ray {
    pub origin: Vec2,
    pub direction: Vec2,
}
