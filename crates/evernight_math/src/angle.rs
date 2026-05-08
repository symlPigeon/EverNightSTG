use std::ops::{Add, Mul, Sub};

use crate::Vec2;

/// Wrapper around f32 to represent an angle in radians.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Angle(pub f32);

impl Angle {
    pub fn from_radians(radians: f32) -> Self {
        Angle(radians)
    }

    pub fn from_degrees(degrees: f32) -> Self {
        Angle(degrees.to_radians())
    }

    pub fn sin(&self) -> f32 {
        self.0.sin()
    }

    pub fn cos(&self) -> f32 {
        self.0.cos()
    }

    pub fn tan(&self) -> f32 {
        self.0.tan()
    }

    pub fn to_direction(self) -> Vec2 {
        Vec2 {
            x: self.cos(),
            y: self.sin(),
        }
    }

    pub fn difference(self, other: Angle) -> Angle {
        self - other
    }

    pub fn normalize(self) -> Angle {
        if !self.0.is_finite() {
            return Angle(self.0);
        }

        if self.0 >= -std::f32::consts::PI && self.0 < std::f32::consts::PI {
            return self;
        }

        let a = (self.0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;

        Angle(a)
    }
}

impl Add for Angle {
    type Output = Angle;
    fn add(self, other: Angle) -> Angle {
        Angle(self.0 + other.0).normalize()
    }
}

impl Sub for Angle {
    type Output = Angle;
    fn sub(self, other: Angle) -> Angle {
        Angle(self.0 - other.0).normalize()
    }
}

impl Mul<f32> for Angle {
    type Output = Angle;
    fn mul(self, scalar: f32) -> Angle {
        Angle(self.0 * scalar).normalize()
    }
}
