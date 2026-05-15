use evernight_core::{Component, EverNightResult};
use evernight_runtime::{FixedStep, Phase, PRIORITY_BUILTIN, StepResult, World};

use crate::engine::ScriptEngine;
use crate::{ComponentRegistry, ScriptContext, TagRegistry, TemplateComponentFn, TemplateRegistry};

/// A user-facing system hook that receives a [`ScriptContext`] each frame phase.
pub type AppHookFn = Box<dyn FnMut(&mut ScriptContext)>;

/// Unified internal system type used for both built-in behaviors and user hooks.
/// Not exposed publicly; callers use [`AppHookFn`] or `Box<dyn FnMut(&mut World)>`.
type AppSystemFn = Box<dyn FnMut(&mut World, &ComponentRegistry, &TagRegistry)>;

/// Top-level application handle for the Evernight engine.
///
/// `App` owns the [`World`], both registries, and all user-registered systems.
/// Call [`App::step()`] once per game tick from your main loop.
///
/// # Example
/// ```rust,ignore
/// let mut app = App::new(FixedStep::new_60hz());
/// app.register_component::<Transform>("Transform", |_| Box::new(Transform::identity()));
/// app.add_system(Phase::PostCollision, PRIORITY_DEFAULT, Box::new(|ctx| {
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
    pre_update:        Vec<(i32, AppSystemFn)>,
    post_spawn_commit:  Vec<(i32, AppSystemFn)>,
    pre_movement:      Vec<(i32, AppSystemFn)>,
    post_movement:     Vec<(i32, AppSystemFn)>,
    pre_collision:     Vec<(i32, AppSystemFn)>,
    post_collision:    Vec<(i32, AppSystemFn)>,
    pre_lifetime:      Vec<(i32, AppSystemFn)>,
    post_lifetime:     Vec<(i32, AppSystemFn)>,
    post_update:       Vec<(i32, AppSystemFn)>,
}

impl App {
    pub fn new(fixed_step: FixedStep) -> Self {
        let mut app = App {
            world: World::new(fixed_step),
            component_registry: ComponentRegistry::new(),
            tag_registry: TagRegistry::new(),
            template_registry: TemplateRegistry::new(),
            script_engine: None,
            pre_update:        Vec::new(),
            post_spawn_commit:  Vec::new(),
            pre_movement:      Vec::new(),
            post_movement:     Vec::new(),
            pre_collision:     Vec::new(),
            post_collision:    Vec::new(),
            pre_lifetime:      Vec::new(),
            post_lifetime:     Vec::new(),
            post_update:       Vec::new(),
        };
        // Register standard built-in behaviors at PRIORITY_BUILTIN so they always
        // run before user systems in the same phase.  Both are no-ops when no
        // entities carry the relevant components.
        app.register_behavior(Phase::PostMovement, PRIORITY_BUILTIN,
            Box::new(|world| world.run_bounded_system()));
        app.register_behavior(Phase::PostCollision, PRIORITY_BUILTIN,
            Box::new(|world| world.run_elastic_collision_system()));
        app
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

    /// Registers a component-driven Rust behavior for the given phase.
    ///
    /// The system receives raw `&mut World` access and is stored alongside user hooks
    /// in a single priority-sorted list.  Use [`PRIORITY_BUILTIN`] so built-in
    /// behaviors run before user systems.
    pub fn register_behavior(&mut self, phase: Phase, priority: i32, system: Box<dyn FnMut(&mut World)>) {
        let mut system = system;
        let app_system: AppSystemFn = Box::new(move |world, _cr, _tr| system(world));
        sorted_insert(self.phase_entries_mut(phase), priority, app_system);
    }

    /// Registers a user hook to run at the given [`Phase`] with the given `priority`.
    ///
    /// Lower priority value → runs earlier.  Use [`PRIORITY_DEFAULT`] for typical
    /// user systems.  Equal-priority hooks run in registration order (FIFO).
    pub fn add_system(&mut self, phase: Phase, priority: i32, hook: AppHookFn) {
        let mut hook = hook;
        let app_system: AppSystemFn = Box::new(move |world, cr, tr| {
            let mut ctx = ScriptContext::new(world, cr, tr);
            hook(&mut ctx);
        });
        sorted_insert(self.phase_entries_mut(phase), priority, app_system);
    }

    fn phase_entries_mut(&mut self, phase: Phase) -> &mut Vec<(i32, AppSystemFn)> {
        match phase {
            Phase::PreUpdate       => &mut self.pre_update,
            Phase::PostSpawnCommit => &mut self.post_spawn_commit,
            Phase::PreMovement     => &mut self.pre_movement,
            Phase::PostMovement    => &mut self.post_movement,
            Phase::PreCollision    => &mut self.pre_collision,
            Phase::PostCollision   => &mut self.post_collision,
            Phase::PreLifetime     => &mut self.pre_lifetime,
            Phase::PostLifetime    => &mut self.post_lifetime,
            Phase::PostUpdate      => &mut self.post_update,
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
    /// Execution order per tick:
    /// 1. `PreUpdate` hooks → event bus cleared
    /// 2. SpawnCommit → `PostSpawnCommit` hooks
    /// 3. `PreMovement` hooks → movement system → `PostMovement` hooks
    /// 4. `PreCollision` hooks → collision system → `PostCollision` hooks
    /// 5. ScriptEngine `on_frame`
    /// 6. `PreLifetime` hooks → lifetime system → `PostLifetime` hooks
    /// 7. `PostUpdate` hooks → tick advanced
    pub fn step(&mut self) -> EverNightResult<StepResult> {
        // 1. PreUpdate
        self.world.clear_events_for_frame();
        run_app_phase(&mut self.pre_update, &mut self.world, &self.component_registry, &self.tag_registry);

        // 2. SpawnCommit
        let factory = |name: &str, data: &[u8]| self.component_registry.create(name, data);
        let tmpl_factory = |id: u32| self.template_registry.instantiate(id);
        self.world
            .commit_commands(Some(&factory), Some(&tmpl_factory))?;
        run_app_phase(&mut self.post_spawn_commit, &mut self.world, &self.component_registry, &self.tag_registry);

        // 3. Movement
        run_app_phase(&mut self.pre_movement, &mut self.world, &self.component_registry, &self.tag_registry);
        self.world.run_movement_system();
        run_app_phase(&mut self.post_movement, &mut self.world, &self.component_registry, &self.tag_registry);

        // 4. Collision
        run_app_phase(&mut self.pre_collision, &mut self.world, &self.component_registry, &self.tag_registry);
        self.world.run_collision_system();
        run_app_phase(&mut self.post_collision, &mut self.world, &self.component_registry, &self.tag_registry);

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

        // 5. PreLifetime
        run_app_phase(&mut self.pre_lifetime, &mut self.world, &self.component_registry, &self.tag_registry);
        self.world.run_lifetime_system()?;
        run_app_phase(&mut self.post_lifetime, &mut self.world, &self.component_registry, &self.tag_registry);

        // 6. PostUpdate
        run_app_phase(&mut self.post_update, &mut self.world, &self.component_registry, &self.tag_registry);

        // 7. Advance tick
        Ok(self.world.advance_tick())
    }

    /// Like [`step`](App::step) but also returns a per-phase timing breakdown.
    ///
    /// Only available when the `benchmark` feature is enabled.
    #[cfg(feature = "benchmark")]
    pub fn step_profiled(
        &mut self,
    ) -> EverNightResult<(StepResult, evernight_benchmark::AppFrameProfile)> {
        use std::time::Instant;
        use evernight_benchmark::AppFrameProfile;

        let mut p = AppFrameProfile::default();

        macro_rules! time_phase {
            ($field:ident, $phase:ident) => {{
                let _t = Instant::now();
                run_app_phase(
                    &mut self.$phase,
                    &mut self.world,
                    &self.component_registry,
                    &self.tag_registry,
                );
                p.$field = _t.elapsed();
            }};
        }

        macro_rules! time {
            ($field:ident, $body:expr) => {{
                let _t = Instant::now();
                $body;
                p.$field = _t.elapsed();
            }};
        }

        // 1. PreUpdate
        time!(pre_update, {
            self.world.clear_events_for_frame();
            run_app_phase(&mut self.pre_update, &mut self.world, &self.component_registry, &self.tag_registry);
        });

        // 2. SpawnCommit
        time!(spawn_commit, {
            let factory = |name: &str, data: &[u8]| self.component_registry.create(name, data);
            let tmpl_factory = |id: u32| self.template_registry.instantiate(id);
            self.world.commit_commands(Some(&factory), Some(&tmpl_factory))?;
        });
        time_phase!(post_spawn_commit, post_spawn_commit);

        // 3. Movement
        time_phase!(pre_movement, pre_movement);
        time!(movement,      self.world.run_movement_system());
        time_phase!(post_movement, post_movement);

        // 4. Collision
        time_phase!(pre_collision, pre_collision);
        time!(collision,     self.world.run_collision_system());
        time_phase!(post_collision, post_collision);

        // ScriptEngine::on_frame
        time!(script_on_frame, {
            if let Some(ref mut engine) = self.script_engine {
                let mut ctx = ScriptContext::new(
                    &mut self.world,
                    &self.component_registry,
                    &self.tag_registry,
                );
                engine.on_frame(&mut ctx)?;
            }
        });

        // 5. Lifetime
        time_phase!(pre_lifetime, pre_lifetime);
        time!(lifetime,      self.world.run_lifetime_system()?);
        time_phase!(post_lifetime, post_lifetime);

        // 6. PostUpdate
        time_phase!(post_update, post_update);

        Ok((self.world.advance_tick(), p))
    }
}

/// Runs every system in `entries` (priority-sorted) with the shared frame resources.
fn run_app_phase(
    entries: &mut Vec<(i32, AppSystemFn)>,
    world: &mut World,
    component_registry: &ComponentRegistry,
    tag_registry: &TagRegistry,
) {
    for (_, system) in entries.iter_mut() {
        system(world, component_registry, tag_registry);
    }
}

/// Inserts `system` into a priority-sorted vec, maintaining FIFO order for equal priorities.
fn sorted_insert(entries: &mut Vec<(i32, AppSystemFn)>, priority: i32, system: AppSystemFn) {
    let pos = entries.partition_point(|e| e.0 <= priority);
    entries.insert(pos, (priority, system));
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use evernight_core::{
        CollisionMask, EventPayload, LayerBit, SpawnRequest, Tick, impl_component,
    };
    use evernight_math::{Angle, Vec2};
    use evernight_math::{Circle, Shape2D};
    use evernight_runtime::{FixedStep, Hitbox, Hurtbox, Lifetime, Phase, PRIORITY_DEFAULT, Transform, Velocity};

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
            PRIORITY_DEFAULT,
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
            PRIORITY_DEFAULT,
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
            PRIORITY_DEFAULT,
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

    type MockAction = Box<dyn FnMut(&mut crate::ScriptContext<'_>) -> EvResult<()>>;

    /// Minimal mock engine: counts `on_frame` calls and optionally runs a closure.
    struct MockEngine {
        frame_count: u32,
        action: Option<MockAction>,
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
            PRIORITY_DEFAULT,
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
            PRIORITY_DEFAULT,
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
