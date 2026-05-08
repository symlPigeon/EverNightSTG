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
