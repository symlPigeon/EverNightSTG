use evernight_core::{Component, EverNightResult};
use evernight_runtime::{FixedStep, Phase, StepResult, World};

use crate::engine::ScriptEngine;
use crate::{ComponentRegistry, ScriptContext, TagRegistry, TemplateComponentFn, TemplateRegistry};

/// A system hook that receives a [`ScriptContext`] each frame phase.
pub type AppHookFn = Box<dyn FnMut(&mut ScriptContext)>;

/// Top-level application handle for the Evernight engine.
///
/// `App` owns the [`World`], both registries, and all user-registered systems.
/// Call [`App::step()`] once per game tick from your main loop.
///
/// # Example
/// ```rust,ignore
/// let mut app = App::new(FixedStep::new_60hz());
/// app.register_component::<Transform>("Transform", |_| Box::new(Transform::identity()));
/// app.add_system(Phase::PostCollision, Box::new(|ctx| {
///     for event in ctx.events() { /* ... */ }
/// }));
/// loop { app.step().unwrap(); }
/// ```
pub struct App {
    world: World,
    component_registry: ComponentRegistry,
    tag_registry: TagRegistry,
    template_registry: TemplateRegistry,
    script_engine: Option<Box<dyn ScriptEngine>>,
    pre_update: Vec<AppHookFn>,
    post_spawn_commit: Vec<AppHookFn>,
    post_movement: Vec<AppHookFn>,
    post_collision: Vec<AppHookFn>,
    post_lifetime: Vec<AppHookFn>,
    post_update: Vec<AppHookFn>,
}

impl App {
    pub fn new(fixed_step: FixedStep) -> Self {
        App {
            world: World::new(fixed_step),
            component_registry: ComponentRegistry::new(),
            tag_registry: TagRegistry::new(),
            template_registry: TemplateRegistry::new(),
            script_engine: None,
            pre_update: Vec::new(),
            post_spawn_commit: Vec::new(),
            post_movement: Vec::new(),
            post_collision: Vec::new(),
            post_lifetime: Vec::new(),
            post_update: Vec::new(),
        }
    }

    // ── Registration ──────────────────────────────────────────────────────────

    /// Registers a component type with a factory function.
    /// The factory receives raw serialized data (`&[u8]`) and returns a boxed component.
    /// Ignoring `data` and returning a default value is valid until serialization is implemented.
    pub fn register_component<F>(&mut self, name: &str, factory: F)
    where
        F: Fn(&[u8]) -> Box<dyn Component> + 'static,
    {
        self.component_registry.register(name, factory);
    }

    /// Registers a custom tag string and returns its numeric ID.
    /// Calling with the same name twice is idempotent.
    pub fn register_tag(&mut self, name: &str) -> u32 {
        self.tag_registry.register(name)
    }

    /// Registers a spawn template and returns its stable `u32` ID.
    ///
    /// A template is a list of component factories invoked once per spawn.  Re-registering
    /// the same name replaces the template but keeps the ID.
    pub fn register_template(&mut self, name: &str, components: Vec<TemplateComponentFn>) -> u32 {
        self.template_registry.register(name, components)
    }

    /// Returns the ID for a previously-registered template name, if it exists.
    pub fn template_id(&self, name: &str) -> Option<u32> {
        self.template_registry.id_of(name)
    }

    /// Attaches a scripting backend.  Replaces any previously set engine.
    pub fn set_script_engine(&mut self, engine: Box<dyn ScriptEngine>) {
        self.script_engine = Some(engine);
    }

    /// Convenience: compile a script source string through the attached engine.
    ///
    /// Returns `Ok(())` silently if no engine has been set.
    pub fn load_script(&mut self, source: &str) -> EverNightResult<()> {
        if let Some(ref mut engine) = self.script_engine {
            engine.load(source)?;
        }
        Ok(())
    }

    /// Registers a system to run at the given [`Phase`] each frame.
    /// Systems within the same phase are called in registration order.
    pub fn add_system(&mut self, phase: Phase, hook: AppHookFn) {
        match phase {
            Phase::PreUpdate => self.pre_update.push(hook),
            Phase::PostSpawnCommit => self.post_spawn_commit.push(hook),
            Phase::PostMovement => self.post_movement.push(hook),
            Phase::PostCollision => self.post_collision.push(hook),
            Phase::PostLifetime => self.post_lifetime.push(hook),
            Phase::PostUpdate => self.post_update.push(hook),
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
    pub fn component_registry(&self) -> &ComponentRegistry {
        &self.component_registry
    }
    pub fn tag_registry(&self) -> &TagRegistry {
        &self.tag_registry
    }

    // ── Frame step ────────────────────────────────────────────────────────────

    /// Advances the simulation by one fixed-step tick.
    ///
    /// Execution order:
    /// 1. PreUpdate — event bus cleared, then user hooks run
    /// 2. SpawnCommit — buffered commands applied, then user hooks run
    /// 3. Movement — velocity integrated into position, then user hooks run
    /// 4. Collision — overlap events emitted, then user hooks run
    /// 5. Lifetime — expired entities despawned, then user hooks run
    /// 6. PostUpdate — final user hooks
    /// 7. Tick counter advanced
    pub fn step(&mut self) -> EverNightResult<StepResult> {
        // 1. PreUpdate
        self.world.clear_events_for_frame();
        run_hooks(
            &mut self.pre_update,
            &mut self.world,
            &self.component_registry,
            &self.tag_registry,
        );

        // 2. SpawnCommit
        let factory = |name: &str, data: &[u8]| self.component_registry.create(name, data);
        let tmpl_factory = |id: u32| self.template_registry.instantiate(id);
        self.world
            .commit_commands(Some(&factory), Some(&tmpl_factory))?;
        run_hooks(
            &mut self.post_spawn_commit,
            &mut self.world,
            &self.component_registry,
            &self.tag_registry,
        );

        // 3. Movement
        self.world.run_movement_system();
        self.world.run_bounded_system();
        run_hooks(
            &mut self.post_movement,
            &mut self.world,
            &self.component_registry,
            &self.tag_registry,
        );

        // 4. Collision
        self.world.run_collision_system();
        self.world.run_elastic_collision_system();
        run_hooks(
            &mut self.post_collision,
            &mut self.world,
            &self.component_registry,
            &self.tag_registry,
        );

        // 7. ScriptEngine::on_frame (after collision, before lifetime so scripts
        //    can still queue despawns that lifetime will clean up this tick)
        if let Some(ref mut engine) = self.script_engine {
            let mut ctx = ScriptContext::new(
                &mut self.world,
                &self.component_registry,
                &self.tag_registry,
            );
            engine.on_frame(&mut ctx)?;
        }

        // 6. Lifetime
        self.world.run_lifetime_system()?;
        run_hooks(
            &mut self.post_lifetime,
            &mut self.world,
            &self.component_registry,
            &self.tag_registry,
        );

        // 8. PostUpdate
        run_hooks(
            &mut self.post_update,
            &mut self.world,
            &self.component_registry,
            &self.tag_registry,
        );

        // 9. Advance tick
        Ok(self.world.advance_tick())
    }
}

fn run_hooks(
    hooks: &mut Vec<AppHookFn>,
    world: &mut World,
    component_registry: &ComponentRegistry,
    tag_registry: &TagRegistry,
) {
    for hook in hooks.iter_mut() {
        let mut ctx = ScriptContext::new(world, component_registry, tag_registry);
        hook(&mut ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use evernight_core::{
        CollisionMask, EventPayload, LayerBit, SpawnRequest, Tick, impl_component,
    };
    use evernight_math::{Angle, Vec2};
    use evernight_math::{Circle, Shape2D};
    use evernight_runtime::{FixedStep, Hitbox, Hurtbox, Lifetime, Phase, Transform, Velocity};

    use super::App;

    fn make_app() -> App {
        App::new(FixedStep::new_60hz())
    }

    fn circle(cx: f32, cy: f32, r: f32) -> Shape2D {
        Shape2D::Circle(Circle {
            center: Vec2::new(cx, cy),
            radius: r,
        })
    }

    // ── Spawn / despawn via ScriptContext ─────────────────────────────────────

    #[test]
    fn ctx_spawn_produces_spawned_event() {
        let mut app = make_app();
        app.add_system(
            Phase::PreUpdate,
            Box::new(|ctx| {
                ctx.spawn(SpawnRequest::new()).unwrap();
            }),
        );
        app.step().unwrap();
        let events = app.world().get_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::Spawned { .. }))
        );
    }

    #[test]
    fn ctx_despawn_removes_entity() {
        let mut app = make_app();
        // Spawn an entity in the first step.
        let entity = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.step().unwrap();
        assert!(app.world().is_alive(entity));

        // Despawn it via a hook in the second step.
        app.add_system(
            Phase::PreUpdate,
            Box::new(move |ctx| {
                ctx.despawn(entity).unwrap();
            }),
        );
        app.step().unwrap();
        assert!(!app.world().is_alive(entity));
    }

    // ── ComponentRegistry integration ─────────────────────────────────────────

    #[test]
    fn spawn_request_with_registered_component_instantiates_it() {
        let mut app = make_app();
        app.register_component("Transform", |_| Box::new(Transform::identity()));

        let request = SpawnRequest::new().add_component("Transform", vec![]);
        let entity = app.world_mut().spawn_entity(request).unwrap();
        app.step().unwrap();

        assert!(app.world().get_component::<Transform>(entity).is_some());
    }

    #[test]
    fn spawn_request_with_unregistered_component_skips_silently() {
        let mut app = make_app();
        // No components registered.
        let request = SpawnRequest::new().add_component("Ghost", vec![]);
        let entity = app.world_mut().spawn_entity(request).unwrap();
        // Should not panic — entity spawns, component is simply absent.
        app.step().unwrap();
        assert!(app.world().is_alive(entity));
    }

    // ── TagRegistry integration ───────────────────────────────────────────────

    #[test]
    fn registered_tag_id_accessible_from_context() {
        let mut app = make_app();
        let id = app.register_tag("invincible");

        let captured = Arc::new(Mutex::new(None::<u32>));
        let captured_clone = Arc::clone(&captured);
        app.add_system(
            Phase::PostUpdate,
            Box::new(move |ctx| {
                *captured_clone.lock().unwrap() = ctx.tag_registry().id_of("invincible");
            }),
        );
        app.step().unwrap();

        assert_eq!(*captured.lock().unwrap(), Some(id));
    }

    // ── ScriptEngine integration ──────────────────────────────────────────────

    use crate::ScriptEngine;
    use evernight_core::EverNightResult as EvResult;

    /// Minimal mock engine: counts `on_frame` calls and optionally runs a closure.
    struct MockEngine {
        frame_count: u32,
        action: Option<Box<dyn FnMut(&mut crate::ScriptContext<'_>) -> EvResult<()>>>,
    }

    impl MockEngine {
        fn new() -> Self {
            MockEngine {
                frame_count: 0,
                action: None,
            }
        }
        fn with_action(
            action: impl FnMut(&mut crate::ScriptContext<'_>) -> EvResult<()> + 'static,
        ) -> Self {
            MockEngine {
                frame_count: 0,
                action: Some(Box::new(action)),
            }
        }
    }

    impl ScriptEngine for MockEngine {
        fn load(&mut self, _source: &str) -> EvResult<()> {
            Ok(())
        }
        fn on_frame(&mut self, ctx: &mut crate::ScriptContext<'_>) -> EvResult<()> {
            self.frame_count += 1;
            if let Some(ref mut f) = self.action {
                f(ctx)?;
            }
            Ok(())
        }
    }

    #[test]
    fn script_engine_on_frame_called_each_step() {
        let mut app = make_app();
        let count = Arc::new(Mutex::new(0u32));
        let count_clone = Arc::clone(&count);
        app.set_script_engine(Box::new(MockEngine::with_action(move |_ctx| {
            *count_clone.lock().unwrap() += 1;
            Ok(())
        })));

        app.step().unwrap();
        app.step().unwrap();
        app.step().unwrap();

        assert_eq!(*count.lock().unwrap(), 3);
    }

    #[test]
    fn script_engine_can_spawn_entity() {
        let mut app = make_app();
        let spawned = Arc::new(Mutex::new(None::<evernight_core::EntityId>));
        let spawned_clone = Arc::clone(&spawned);

        app.set_script_engine(Box::new(MockEngine::with_action(move |ctx| {
            if spawned_clone.lock().unwrap().is_none() {
                let id = ctx.spawn(SpawnRequest::new())?;
                *spawned_clone.lock().unwrap() = Some(id);
            }
            Ok(())
        })));

        app.step().unwrap(); // on_frame runs, spawns entity (committed next step)
        let id = spawned.lock().unwrap().unwrap();
        assert!(app.world().is_alive(id));
    }

    #[test]
    fn script_engine_sees_collision_events() {
        let mut app = make_app();

        let att = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(
                att,
                Hitbox::new(
                    circle(0.0, 0.0, 1.0),
                    LayerBit::new(0),
                    CollisionMask::new(1),
                    false,
                ),
            )
            .unwrap();
        let def = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(def, Hurtbox::new(circle(0.5, 0.0, 1.0), LayerBit::new(1)))
            .unwrap();

        let hit = Arc::new(Mutex::new(false));
        let hit_clone = Arc::clone(&hit);
        app.set_script_engine(Box::new(MockEngine::with_action(move |ctx| {
            if ctx
                .events()
                .iter()
                .any(|e| matches!(e, EventPayload::Collision { .. }))
            {
                *hit_clone.lock().unwrap() = true;
            }
            Ok(())
        })));

        app.step().unwrap();
        assert!(
            *hit.lock().unwrap(),
            "script engine should see Collision event"
        );
    }

    #[test]
    fn load_script_no_engine_is_noop() {
        // load_script without a set engine must not panic.
        let mut app = make_app();
        assert!(app.load_script("-- no engine attached").is_ok());
    }

    #[test]
    fn load_script_delegates_to_engine() {
        let mut app = make_app();
        app.set_script_engine(Box::new(MockEngine::new()));
        // MockEngine::load always returns Ok; just verify no error.
        assert!(app.load_script("return 1 + 1").is_ok());
    }

    // ── SpawnTemplate via App ─────────────────────────────────────────────────

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Speed(f32);
    impl_component!(Speed);

    #[test]
    fn template_spawn_adds_components() {
        let mut app = make_app();
        let tmpl_id = app.register_template(
            "fast_bullet",
            vec![
                Box::new(|| Box::new(Transform::identity()) as Box<dyn evernight_core::Component>),
                Box::new(|| Box::new(Speed(9.0)) as Box<dyn evernight_core::Component>),
            ],
        );

        let req = SpawnRequest::with_template(tmpl_id);
        let entity = app.world_mut().spawn_entity(req).unwrap();
        app.step().unwrap();

        assert!(app.world().get_component::<Transform>(entity).is_some());
        let speed = app.world().get_component::<Speed>(entity);
        assert_eq!(speed, Some(&Speed(9.0)));
    }

    #[test]
    fn template_id_roundtrip() {
        let mut app = make_app();
        let id = app.register_template("bullet", vec![]);
        assert_eq!(app.template_id("bullet"), Some(id));
        assert_eq!(app.template_id("missing"), None);
    }

    #[test]
    fn unknown_template_id_spawns_empty_entity() {
        let mut app = make_app();
        // Spawn with a template ID that was never registered.
        let req = SpawnRequest::with_template(999);
        let entity = app.world_mut().spawn_entity(req).unwrap();
        app.step().unwrap(); // must not panic
        // Entity is alive but has no components (other than what we might query)
        assert!(app.world().is_alive(entity));
        assert!(app.world().get_component::<Transform>(entity).is_none());
    }

    #[test]
    fn template_plus_named_components_combined() {
        let mut app = make_app();
        app.register_component("Speed", |_| Box::new(Speed(5.0)));
        let tmpl_id = app.register_template(
            "base",
            vec![Box::new(|| {
                Box::new(Transform::identity()) as Box<dyn evernight_core::Component>
            })],
        );

        let req = SpawnRequest::with_template(tmpl_id).add_component("Speed", vec![]);
        let entity = app.world_mut().spawn_entity(req).unwrap();
        app.step().unwrap();

        assert!(app.world().get_component::<Transform>(entity).is_some());
        assert_eq!(
            app.world().get_component::<Speed>(entity),
            Some(&Speed(5.0))
        );
    }

    // ── Movement via App::step ────────────────────────────────────────────────

    #[test]
    fn movement_runs_inside_app_step() {
        let mut app = make_app();
        let dt = app.world().delta_time();
        let entity = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(entity, Transform::identity())
            .unwrap();
        app.world_mut()
            .add_component(entity, Velocity::new(Vec2::new(10.0, 0.0), Angle(0.0)))
            .unwrap();
        app.step().unwrap();

        let x = app
            .world()
            .get_component::<Transform>(entity)
            .unwrap()
            .position
            .x;
        assert!((x - 10.0 * dt).abs() < 1e-5);
    }

    // ── Collision event visible in PostCollision hook ─────────────────────────

    #[test]
    fn post_collision_hook_sees_collision_event() {
        let mut app = make_app();

        let att = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(
                att,
                Hitbox::new(
                    circle(0.0, 0.0, 1.0),
                    LayerBit::new(0),
                    CollisionMask::new(1),
                    false,
                ),
            )
            .unwrap();

        let def = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(def, Hurtbox::new(circle(0.5, 0.0, 1.0), LayerBit::new(1)))
            .unwrap();

        let hit = Arc::new(Mutex::new(false));
        let hit_clone = Arc::clone(&hit);
        app.add_system(
            Phase::PostCollision,
            Box::new(move |ctx| {
                if ctx
                    .events()
                    .iter()
                    .any(|e| matches!(e, EventPayload::Collision { .. }))
                {
                    *hit_clone.lock().unwrap() = true;
                }
            }),
        );

        app.step().unwrap();
        assert!(
            *hit.lock().unwrap(),
            "PostCollision hook should see Collision event"
        );
    }

    // ── Lifetime expiry via App::step ─────────────────────────────────────────

    #[test]
    fn lifetime_expiry_despawns_entity() {
        let mut app = make_app();
        let entity = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(entity, Lifetime::new(Tick::new(1)))
            .unwrap();
        app.step().unwrap();
        assert!(!app.world().is_alive(entity));
    }

    // ── Hook spawn during frame (commands applied next SpawnCommit) ───────────

    #[test]
    fn hook_spawned_entity_visible_after_next_step() {
        let mut app = make_app();
        let spawned = Arc::new(Mutex::new(None::<evernight_core::EntityId>));
        let spawned_clone = Arc::clone(&spawned);

        app.add_system(
            Phase::PostUpdate,
            Box::new(move |ctx| {
                // Only spawn once (when spawned is still None).
                if spawned_clone.lock().unwrap().is_none() {
                    let id = ctx.spawn(SpawnRequest::new()).unwrap();
                    *spawned_clone.lock().unwrap() = Some(id);
                }
            }),
        );

        app.step().unwrap(); // hook queues spawn, commit hasn't run yet
        let id = spawned.lock().unwrap().unwrap();
        // Entity is pre-allocated but not committed yet — is_alive checks IdAllocator,
        // which allocated the ID in spawn_entity, so it should already be alive.
        assert!(app.world().is_alive(id));

        app.step().unwrap(); // SpawnCommit runs, Spawned event emitted
        let events = app.world().get_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EventPayload::Spawned { entity, .. } if *entity == id))
        );
    }
}
