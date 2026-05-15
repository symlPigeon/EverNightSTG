use evernight_core::{EntityId, impl_component};

use crate::{ComponentStorage, Transform, Velocity};

/// Keeps an entity inside a rectangular region centred on the world origin.
///
/// The bounds are specified as half-extents: an entity is clamped to the range
/// `[-half_width, half_width]` × `[-half_height, half_height]`.
///
/// When the entity hits a wall its velocity component perpendicular to that
/// wall is reflected so it bounces back.  Entities missing `Transform` or
/// `Velocity` are silently skipped.
#[derive(Debug, Clone, Copy)]
pub struct Bounded {
    pub half_width: f32,
    pub half_height: f32,
}

impl Bounded {
    pub fn new(half_width: f32, half_height: f32) -> Self {
        Self {
            half_width,
            half_height,
        }
    }
}

impl_component!(Bounded);

/// Clamps entities with `Bounded` inside their declared region and reflects
/// the perpendicular velocity component on contact.
///
/// Runs after `movement_system` so that position has already been integrated.
pub fn bounded_system(storage: &mut ComponentStorage) {
    let ids: Vec<EntityId> = storage.iter::<Bounded>().map(|(id, _)| id).collect();

    for id in ids {
        let Some(bounds) = storage.get::<Bounded>(id).copied() else {
            continue;
        };
        let Some(tf) = storage.get::<Transform>(id).copied() else {
            continue;
        };
        let Some(vel) = storage.get::<Velocity>(id).copied() else {
            continue;
        };

        let mut new_tf = tf;
        let mut new_vel = vel;
        let mut changed = false;

        let hw = bounds.half_width;
        let hh = bounds.half_height;

        if new_tf.position.x < -hw {
            new_tf.position.x = -hw;
            new_vel.linear.x = new_vel.linear.x.abs();
            changed = true;
        } else if new_tf.position.x > hw {
            new_tf.position.x = hw;
            new_vel.linear.x = -new_vel.linear.x.abs();
            changed = true;
        }

        if new_tf.position.y < -hh {
            new_tf.position.y = -hh;
            new_vel.linear.y = new_vel.linear.y.abs();
            changed = true;
        } else if new_tf.position.y > hh {
            new_tf.position.y = hh;
            new_vel.linear.y = -new_vel.linear.y.abs();
            changed = true;
        }

        if changed {
            *storage.get_mut::<Transform>(id).unwrap() = new_tf;
            *storage.get_mut::<Velocity>(id).unwrap() = new_vel;
        }
    }
}
