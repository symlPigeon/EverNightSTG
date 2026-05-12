use evernight_core::{CollisionMask, EntityId, EventPayload, LayerBit, Tick};
use evernight_math::Shape2D;

use crate::{ComponentStorage, EventBus, Hitbox, Hurtbox, Lifetime, Transform, Velocity};

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
/// 1. Collect all hitboxes and hurtboxes (clone to avoid simultaneous borrows).
/// 2. Broad-phase: filter pairs by `hitbox.group.collides_with(hurtbox.layer)`.
/// 3. Narrow-phase: `evernight_math::detect()` per candidate pair.
/// 4. Sort confirmed hits by `(attacker, defender)` for deterministic replay.
/// 5. Push events; `hit_once` hitboxes stop after their first hit.
pub fn collision_system(
    storage: &mut ComponentStorage,
    event_bus: &mut EventBus,
    tick: Tick,
) {
    use evernight_math::{Vec2, detect};

    // Snapshot hitbox and hurtbox data so we can iterate both without aliasing.
    let hitboxes: Vec<(EntityId, Shape2D, CollisionMask, bool)> = storage
        .iter::<Hitbox>()
        .map(|(id, h)| (id, h.shape.clone(), h.group, h.hit_once))
        .collect();

    let hurtboxes: Vec<(EntityId, Shape2D, LayerBit)> = storage
        .iter::<Hurtbox>()
        .map(|(id, h)| (id, h.shape.clone(), h.layer))
        .collect();

    // Collect confirmed hits before emitting events.
    // BTreeMap iteration already yields attacker IDs in ascending order;
    // sorting by (attacker, defender) makes the full sequence deterministic.
    let mut hits: Vec<(EntityId, EntityId, Vec2, Vec2)> = Vec::new();

    for (att_id, att_shape, att_group, att_hit_once) in &hitboxes {
        let mut fired = false;
        for (def_id, def_shape, def_layer) in &hurtboxes {
            if att_id == def_id {
                continue; // no self-collision
            }
            if !att_group.collides_with(*def_layer) {
                continue; // broad-phase mask filter
            }
            let result = detect(att_shape, def_shape);
            if result.is_colliding {
                let contact = result.contact_point.unwrap_or(Vec2::zero());
                let normal = result.normal.unwrap_or(Vec2::zero());
                hits.push((*att_id, *def_id, contact, normal));
                if *att_hit_once {
                    fired = true;
                    break;
                }
            }
        }
        let _ = fired; // used only for the break above
    }

    // Sort for deterministic replay before emitting.
    hits.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    for (attacker, defender, contact_point, normal) in hits {
        event_bus.push(EventPayload::Collision {
            attacker,
            defender,
            contact_point,
            normal,
            tick,
        });
    }
}
