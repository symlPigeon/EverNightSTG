use crate::{Angle, Vec2};

/// 2D transform for position, rotation, and scale.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform2D {
    pub position: Vec2,
    pub rotation: Angle,
    pub scale: Vec2,
}

impl Transform2D {
    pub fn new(position: Vec2, rotation: Angle) -> Self {
        Transform2D {
            position,
            rotation,
            scale: Vec2::new(1.0, 1.0),
        }
    }

    pub fn identity() -> Self {
        Transform2D {
            position: Vec2::zero(),
            rotation: Angle(0.0),
            scale: Vec2::new(1.0, 1.0),
        }
    }

    pub fn apply_to_point(&self, point: Vec2) -> Vec2 {
        let scaled = Vec2 {
            x: point.x * self.scale.x,
            y: point.y * self.scale.y,
        };
        let rotated = scaled.rotated(self.rotation.0);
        Vec2 {
            x: rotated.x + self.position.x,
            y: rotated.y + self.position.y,
        }
    }

    pub fn apply_to_direction(&self, direction: Vec2) -> Vec2 {
        // Only rotate, don't scale
        direction.rotated(self.rotation.0)
    }

    pub fn inverse(&self) -> Self {
        let inv_scale = Vec2 {
            x: if self.scale.x != 0.0 {
                1.0 / self.scale.x
            } else {
                0.0
            },
            y: if self.scale.y != 0.0 {
                1.0 / self.scale.y
            } else {
                0.0
            },
        };
        let inv_rotation = Angle(-self.rotation.0).normalize();
        // Inverse: translate first, then rotate inv, then scale inv
        let temp = Vec2 {
            x: -self.position.x,
            y: -self.position.y,
        }
        .rotated(inv_rotation.0);
        let inv_position = Vec2 {
            x: temp.x * inv_scale.x,
            y: temp.y * inv_scale.y,
        };
        Transform2D {
            position: inv_position,
            rotation: inv_rotation,
            scale: inv_scale,
        }
    }
}
