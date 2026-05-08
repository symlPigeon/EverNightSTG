use std::ops::{Add, Div, Mul, Neg, Sub};

/// 2D vector for mathematical operations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }

    pub fn zero() -> Self {
        Vec2 { x: 0.0, y: 0.0 }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn normalized(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Vec2 {
                x: self.x / len,
                y: self.y / len,
            }
        } else {
            Vec2::zero()
        }
    }

    pub fn dot(&self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn cross(&self, other: Vec2) -> f32 {
        self.x * other.y - self.y * other.x
    }

    pub fn rotated(&self, angle_rad: f32) -> Self {
        let cos = angle_rad.cos();
        let sin = angle_rad.sin();
        Vec2 {
            x: self.x * cos - self.y * sin,
            y: self.x * sin + self.y * cos,
        }
    }

    pub fn angle_to(&self, other: Vec2) -> f32 {
        self.dot(other).atan2(self.cross(other))
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, other: Vec2) -> Vec2 {
        Vec2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, scalar: f32) -> Vec2 {
        Vec2 {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

impl Div<f32> for Vec2 {
    type Output = Vec2;
    fn div(self, scalar: f32) -> Vec2 {
        debug_assert_ne!(scalar, 0.0, "[vec2] division by zero");
        Vec2 {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2 {
            x: -self.x,
            y: -self.y,
        }
    }
}

pub const VEC2_UP: Vec2 = Vec2 { x: 0.0, y: 1.0 };
pub const VEC2_DOWN: Vec2 = Vec2 { x: 0.0, y: -1.0 };
pub const VEC2_LEFT: Vec2 = Vec2 { x: -1.0, y: 0.0 };
pub const VEC2_RIGHT: Vec2 = Vec2 { x: 1.0, y: 0.0 };
pub const VEC2_ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
