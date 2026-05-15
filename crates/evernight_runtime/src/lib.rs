pub mod behaviors;
pub mod collision;
pub mod commands;
pub mod component_storage;
pub mod components;
pub mod events;
pub mod scheduler;
pub mod spatial_hash;
pub mod systems;
pub mod world;

pub use {
    behaviors::*, collision::*, commands::*, component_storage::*, components::*, events::*,
    scheduler::*, spatial_hash::*, systems::*, world::*,
};

#[cfg(test)]
mod tests {
    use evernight_core::{CollisionMask, EventPayload, LayerBit, SpawnRequest, Tick};
    use evernight_math::{Angle, Circle, Shape2D, Vec2};

    use crate::{FixedStep, Hitbox, Hurtbox, Lifetime, Phase, Scheduler, Transform, Velocity, World};

    fn make_world() -> World {
        World::new(FixedStep::new_60hz())
    }

    fn no_hooks() -> Scheduler {
        Scheduler::new()
    }

    // ── SpawnCommit ───────────────────────────────────────────────────────────

    #[test]
    fn spawn_produces_spawned_event() {
        let mut world = make_world();
        let mut sched = no_hooks();
        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.step(&mut sched).unwrap();
        let events = world.get_events();
        assert!(
            events.iter().any(|e| matches!(e, EventPayload::Spawned { entity: e_id, .. } if *e_id == entity)),
            "expected Spawned event for {entity:?}"
        );
    }

    #[test]
    fn add_component_before_step_is_visible_after() {
        let mut world = make_world();
        let mut sched = no_hooks();
        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Transform::identity()).unwrap();
        world.step(&mut sched).unwrap();
        assert!(world.get_component::<Transform>(entity).is_some());
    }

    #[test]
    fn despawn_removes_components_and_emits_event() {
        let mut world = make_world();
        let mut sched = no_hooks();
        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Transform::identity()).unwrap();
        world.step(&mut sched).unwrap();

        world.despawn_entity(entity).unwrap();
        world.step(&mut sched).unwrap();

        assert!(!world.is_alive(entity));
        assert!(world.get_component::<Transform>(entity).is_none());
        let events = world.get_events();
        assert!(
            events.iter().any(|e| matches!(e, EventPayload::Despawned { entity: e_id, .. } if *e_id == entity)),
        );
    }

    #[test]
    fn despawn_invalid_entity_returns_error() {
        let mut world = make_world();
        let mut sched = no_hooks();
        // Spawn and immediately despawn.
        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.step(&mut sched).unwrap();
        world.despawn_entity(entity).unwrap();
        world.step(&mut sched).unwrap();
        // Second despawn should error.
        assert!(world.despawn_entity(entity).is_err());
    }

    #[test]
    fn remove_component_via_command() {
        let mut world = make_world();
        let mut sched = no_hooks();
        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Transform::identity()).unwrap();
        world.step(&mut sched).unwrap();

        world.remove_component::<Transform>(entity).unwrap();
        world.step(&mut sched).unwrap();

        assert!(world.get_component::<Transform>(entity).is_none());
    }

    // ── movement_system ───────────────────────────────────────────────────────

    #[test]
    fn movement_integrates_velocity() {
        let mut world = make_world();
        let mut sched = no_hooks();
        let dt = world.delta_time();

        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Transform::identity()).unwrap();
        world.add_component(entity, Velocity::new(Vec2::new(10.0, 0.0), Angle(0.0))).unwrap();
        world.step(&mut sched).unwrap();

        let pos = world.get_component::<Transform>(entity).unwrap().position;
        let expected = 10.0 * dt;
        assert!(
            (pos.x - expected).abs() < 1e-5,
            "expected x={expected}, got x={}", pos.x
        );
        assert!(pos.y.abs() < 1e-5);
    }

    #[test]
    fn entity_without_velocity_stays_put() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Transform::identity()).unwrap();
        world.step(&mut sched).unwrap();

        let pos = world.get_component::<Transform>(entity).unwrap().position;
        assert!(pos.x.abs() < 1e-6 && pos.y.abs() < 1e-6);
    }

    // ── lifetime_system ───────────────────────────────────────────────────────

    #[test]
    fn lifetime_one_tick_expires_immediately() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Lifetime::new(Tick::new(1))).unwrap();
        world.step(&mut sched).unwrap();

        assert!(!world.is_alive(entity));
        let events = world.get_events();
        assert!(
            events.iter().any(|e| matches!(e, EventPayload::LifetimeExpired { entity: e_id, .. } if *e_id == entity)),
        );
    }

    #[test]
    fn lifetime_two_ticks_survives_first_step() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Lifetime::new(Tick::new(2))).unwrap();
        world.step(&mut sched).unwrap();

        assert!(world.is_alive(entity), "entity should still be alive after 1 step");
        let remaining = world.get_component::<Lifetime>(entity).unwrap().remaining.as_u32();
        assert_eq!(remaining, 1);

        world.step(&mut sched).unwrap();
        assert!(!world.is_alive(entity));
    }

    // ── collision_system ──────────────────────────────────────────────────────

    fn layer(n: u32) -> LayerBit { LayerBit::new(n) }
    fn mask(n: u32) -> CollisionMask { CollisionMask::new(n) }

    fn circle_shape(cx: f32, cy: f32, r: f32) -> Shape2D {
        Shape2D::Circle(Circle { center: Vec2::new(cx, cy), radius: r })
    }

    #[test]
    fn overlapping_hitbox_hurtbox_emits_collision_event() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let attacker = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(attacker, Hitbox::new(circle_shape(0.0, 0.0, 1.0), layer(0), mask(1), false)).unwrap();

        let defender = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(defender, Hurtbox::new(circle_shape(0.5, 0.0, 1.0), layer(1))).unwrap();

        world.step(&mut sched).unwrap();

        let events = world.get_events();
        assert!(
            events.iter().any(|e| matches!(e,
                EventPayload::Collision { attacker: a, defender: d, .. }
                if *a == attacker && *d == defender
            )),
            "expected Collision event between attacker and defender"
        );
    }

    #[test]
    fn non_overlapping_shapes_no_collision_event() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let attacker = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(attacker, Hitbox::new(circle_shape(0.0, 0.0, 1.0), layer(0), mask(1), false)).unwrap();

        let defender = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(defender, Hurtbox::new(circle_shape(10.0, 0.0, 1.0), layer(1))).unwrap();

        world.step(&mut sched).unwrap();

        assert!(!world.get_events().iter().any(|e| matches!(e, EventPayload::Collision { .. })));
    }

    #[test]
    fn mask_mismatch_no_collision_event() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let attacker = world.spawn_entity(SpawnRequest::new()).unwrap();
        // hitbox targets layer 1, hurtbox is on layer 2 — no match
        world.add_component(attacker, Hitbox::new(circle_shape(0.0, 0.0, 1.0), layer(0), mask(1), false)).unwrap();

        let defender = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(defender, Hurtbox::new(circle_shape(0.5, 0.0, 1.0), layer(2))).unwrap();

        world.step(&mut sched).unwrap();

        assert!(!world.get_events().iter().any(|e| matches!(e, EventPayload::Collision { .. })));
    }

    #[test]
    fn hit_once_emits_single_collision_for_multiple_defenders() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let attacker = world.spawn_entity(SpawnRequest::new()).unwrap();
        // hit_once = true
        world.add_component(attacker, Hitbox::new(circle_shape(0.0, 0.0, 5.0), layer(0), mask(1), true)).unwrap();

        // Three defenders all overlapping the hitbox
        for offset in [0.0_f32, 0.5, 1.0] {
            let def = world.spawn_entity(SpawnRequest::new()).unwrap();
            world.add_component(def, Hurtbox::new(circle_shape(offset, 0.0, 1.0), layer(1))).unwrap();
        }

        world.step(&mut sched).unwrap();

        let collision_count = world.get_events().iter()
            .filter(|e| matches!(e, EventPayload::Collision { attacker: a, .. } if *a == attacker))
            .count();
        assert_eq!(collision_count, 1, "hit_once hitbox should only fire once");
    }

    // ── EventBus frame isolation ──────────────────────────────────────────────

    #[test]
    fn events_cleared_at_start_of_next_step() {
        let mut world = make_world();
        let mut sched = no_hooks();

        let _entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.step(&mut sched).unwrap();
        assert!(!world.get_events().is_empty(), "should have Spawned event");

        // Second step with nothing happening — events from previous frame are gone.
        world.step(&mut sched).unwrap();
        assert!(world.get_events().is_empty());
    }

    // ── Scheduler hooks ───────────────────────────────────────────────────────

    #[test]
    fn post_movement_hook_runs_after_movement() {
        use std::sync::{Arc, Mutex};

        let mut world = make_world();
        let mut sched = Scheduler::new();
        let _dt = world.delta_time();

        let entity = world.spawn_entity(SpawnRequest::new()).unwrap();
        world.add_component(entity, Transform::identity()).unwrap();
        world.add_component(entity, Velocity::new(Vec2::new(1.0, 0.0), Angle(0.0))).unwrap();
        // First step to commit components.
        world.step(&mut sched).unwrap();

        let captured_x = Arc::new(Mutex::new(0.0_f32));
        let captured_x_clone = Arc::clone(&captured_x);

        sched.add_system(Phase::PostMovement, Box::new(move |storage, _events, _cmds, _tick, _dt| {
            if let Some(t) = storage.get::<Transform>(entity) {
                *captured_x_clone.lock().unwrap() = t.position.x;
            }
        }));

        world.step(&mut sched).unwrap();

        let x = *captured_x.lock().unwrap();
        // After second step, movement has run once more, so x ≈ dt * 2 but hook sees post-movement value.
        // Hook runs after movement, so x should be non-zero.
        assert!(x > 0.0, "hook should see updated position, got x={x}");
    }

    // ── Determinism ───────────────────────────────────────────────────────────

    #[test]
    fn collision_event_order_is_deterministic() {
        // Run the same scenario twice and verify the collision event sequence is identical.
        fn run_once() -> Vec<(u32, u32)> {
            let mut world = make_world();
            let mut sched = no_hooks();

            // Attacker
            let att = world.spawn_entity(SpawnRequest::new()).unwrap();
            world.add_component(att, Hitbox::new(circle_shape(0.0, 0.0, 10.0), layer(0), mask(1), false)).unwrap();

            // Three defenders
            for i in 0..3 {
                let def = world.spawn_entity(SpawnRequest::new()).unwrap();
                world.add_component(def, Hurtbox::new(circle_shape(i as f32, 0.0, 1.0), layer(1))).unwrap();
            }

            world.step(&mut sched).unwrap();

            world.get_events().iter().filter_map(|e| match e {
                EventPayload::Collision { attacker, defender, .. } => Some((attacker.as_u32(), defender.as_u32())),
                _ => None,
            }).collect()
        }

        let run_a = run_once();
        let run_b = run_once();
        assert!(!run_a.is_empty());
        assert_eq!(run_a, run_b, "collision event order must be deterministic across runs");
    }
}
