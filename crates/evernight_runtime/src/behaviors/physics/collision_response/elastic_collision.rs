use std::collections::HashSet;

use evernight_core::{EntityId, impl_component};
use evernight_math::Vec2;

use crate::{ComponentStorage, Velocity};

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
