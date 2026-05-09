use evernight_core::{EntityId, EventPayload, Tick};

use crate::{ComponentStorage, EventBus, Lifetime, Transform, Velocity};

/// Integrates `Velocity` into `Transform` for all entities that have both components.
///
/// Position and rotation are updated using Euler integration over `delta_time`.
/// Entities missing either component are silently skipped.
pub fn movement_system(storage: &mut ComponentStorage, delta_time: f32) {
    // Collect velocity data first to avoid a simultaneous mutable + immutable borrow.
    let updates: Vec<(EntityId, Velocity)> = storage
        .iter::<Velocity>()
        .map(|(id, v)| (id, *v))
        .collect();

    for (id, vel) in updates {
        if let Some(transform) = storage.get_mut::<Transform>(id) {
            transform.position = transform.position + vel.linear * delta_time;
            transform.rotation = transform.rotation + vel.angular * delta_time;
        }
    }
}

/// Decrements all `Lifetime` components by one tick.
///
/// Entities whose `remaining` reaches zero are collected, a `LifetimeExpired` event is pushed
/// for each, and their `EntityId`s are returned so the caller can despawn them.
pub fn lifetime_system(
    storage: &mut ComponentStorage,
    event_bus: &mut EventBus,
    tick: Tick,
) -> Vec<EntityId> {
    let all_ids: Vec<EntityId> = storage.iter::<Lifetime>().map(|(id, _)| id).collect();

    let mut expired = Vec::new();
    for id in all_ids {
        if let Some(l) = storage.get_mut::<Lifetime>(id) {
            if l.remaining.as_u32() > 0 {
                l.remaining = Tick::new(l.remaining.as_u32() - 1);
            }
            if l.remaining.as_u32() == 0 {
                expired.push(id);
            }
        }
    }

    for &entity in &expired {
        event_bus.push(EventPayload::LifetimeExpired { entity, tick });
    }

    expired
}

/// Runs broad-phase and narrow-phase collision detection between `Hitbox` and `Hurtbox` pairs,
/// emitting `EventPayload::Collision` events for each overlapping pair.
///
/// Execution order:
/// 1. Broad-phase: filter candidate pairs by `CollisionMask` vs `LayerBit`
/// 2. Narrow-phase: SAT/OBB detection via `evernight_math::detect()`
/// 3. Emit events; results are sorted before emission for deterministic replay.
pub fn collision_system(
    _storage: &mut ComponentStorage,
    _event_bus: &mut EventBus,
    _tick: Tick,
) {
    // TODO: implement after Hitbox/Hurtbox iteration helpers are in place
}
