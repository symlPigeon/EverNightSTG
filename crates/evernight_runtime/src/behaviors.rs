use std::collections::HashSet;

use evernight_core::{EntityId, impl_component};
use evernight_math::Vec2;

use crate::{ComponentStorage, Transform, Velocity};

// ── ElasticCollision ─────────────────────────────────────────────────────────

/// Marks an entity as participating in elastic collision response.
///
/// When two entities that both carry `ElasticCollision` collide, the
/// built-in `elastic_collision_system` applies an impulse to their
/// `Velocity` components in Rust — no Lua `on_collision` callback needed.
///
/// `restitution` is the coefficient of restitution (1.0 = perfectly elastic,
/// 0.0 = perfectly inelastic). The effective restitution for a pair is the
/// average of the two entities' values.
#[derive(Debug, Clone, Copy)]
pub struct ElasticCollision {
    pub restitution: f32,
}

impl ElasticCollision {
    pub fn new(restitution: f32) -> Self {
        Self { restitution }
    }

    pub fn elastic() -> Self {
        Self { restitution: 1.0 }
    }
}

impl_component!(ElasticCollision);

// ── Bounded ──────────────────────────────────────────────────────────────────

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

// ── elastic_collision_system ──────────────────────────────────────────────────

/// Applies elastic impulses between pairs of entities that both carry
/// `ElasticCollision`.
///
/// `pairs` should be the `(attacker, defender, normal)` tuples extracted from
/// the current frame's `Collision` events.  Each canonical pair `(min, max)` is
/// processed at most once per call to avoid double-impulse from symmetric events.
pub fn elastic_collision_system(
    storage: &mut ComponentStorage,
    pairs: &[(EntityId, EntityId, Vec2)],
) {
    // Canonical pair key packed into a u64 for O(1) HashSet lookup.
    let mut seen: HashSet<u64> = HashSet::new();

    for &(att, def, normal) in pairs {
        // Check components first — avoids touching `seen` for entities that have
        // no ElasticCollision (the common case for pure collision-detection benchmarks).
        let rest_a = storage.get::<ElasticCollision>(att).map(|e| e.restitution);
        let rest_b = storage.get::<ElasticCollision>(def).map(|e| e.restitution);
        let (Some(rest_a), Some(rest_b)) = (rest_a, rest_b) else {
            continue;
        };

        // Deduplicate symmetric pairs only for entities that actually participate.
        let lo = att.as_u32().min(def.as_u32()) as u64;
        let hi = att.as_u32().max(def.as_u32()) as u64;
        if !seen.insert(lo | (hi << 32)) {
            continue;
        }

        let va = storage.get::<Velocity>(att).copied();
        let vb = storage.get::<Velocity>(def).copied();
        let (Some(va), Some(vb)) = (va, vb) else {
            continue;
        };

        let rel = (va.linear.x - vb.linear.x) * normal.x + (va.linear.y - vb.linear.y) * normal.y;
        if rel <= 0.0 {
            continue; // already separating
        }

        let j = rel * (rest_a + rest_b) * 0.5;

        if let Some(v) = storage.get_mut::<Velocity>(att) {
            v.linear.x -= j * normal.x;
            v.linear.y -= j * normal.y;
        }
        if let Some(v) = storage.get_mut::<Velocity>(def) {
            v.linear.x += j * normal.x;
            v.linear.y += j * normal.y;
        }
    }
}

// ── bounded_system ────────────────────────────────────────────────────────────

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
