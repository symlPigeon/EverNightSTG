use evernight_core::{Component, EntityId, EventPayload, EverNightError, EverNightResult, IdAllocator, SpawnRequest, TagFlags, Tick};

use crate::{
    Command, CommandBuffer, ComponentStorage, EventBus, Scheduler, SystemEntry, Tag,
    bounded_system, collision_system, elastic_collision_system, lifetime_system, movement_system,
};

/// Runs every [`SystemEntry`] in `entries` in priority order, passing the common frame resources.
fn run_phase(
    entries: &mut Vec<SystemEntry>,
    storage: &mut ComponentStorage,
    event_bus: &mut EventBus,
    cmd_buf: &mut CommandBuffer,
    tick: Tick,
    dt: f32,
) {
    for entry in entries.iter_mut() {
        (entry.system)(storage, event_bus, cmd_buf, tick, dt);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedStep {
    pub delta_time: f32,
}

impl FixedStep {
    pub fn new(delta_time: f32) -> Self {
        FixedStep { delta_time }
    }

    pub fn new_60hz() -> Self {
        FixedStep {
            delta_time: 1.0 / 60.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StepResult {
    pub tick: Tick,
    pub event_count: usize,
}

pub struct World {
    id_allocator: IdAllocator,
    component_storage: ComponentStorage,
    command_buffer: CommandBuffer,
    event_bus: EventBus,
    tick: Tick,
    fixed_step: FixedStep,
}

impl World {
    pub fn new(fixed_step: FixedStep) -> Self {
        World {
            id_allocator: IdAllocator::new(),
            component_storage: ComponentStorage::new(),
            command_buffer: CommandBuffer::new(),
            event_bus: EventBus::new(),
            tick: Tick::new(0),
            fixed_step,
        }
    }

    pub fn tick(&self) -> Tick {
        self.tick
    }

    pub fn delta_time(&self) -> f32 {
        self.fixed_step.delta_time
    }

    pub fn spawn_entity(&mut self, request: SpawnRequest) -> EverNightResult<EntityId> {
        let entity = self.id_allocator.allocate()?;
        self.command_buffer.push(Command::Spawn { entity, request });
        Ok(entity)
    }

    pub fn despawn_entity(&mut self, entity: EntityId) -> EverNightResult<()> {
        if !self.id_allocator.is_valid(entity) {
            return Err(EverNightError::InvalidEntityId(entity));
        }
        self.command_buffer.push(Command::Despawn(entity));
        Ok(())
    }

    pub fn add_component<T: Component>(&mut self, entity: EntityId, component: T) -> EverNightResult<()> {
        if !self.id_allocator.is_valid(entity) {
            return Err(EverNightError::InvalidEntityId(entity));
        }
        self.command_buffer.push(Command::AddComponent { entity, component: Box::new(component) });
        Ok(())
    }

    pub fn get_component<T: Component>(&self, entity: EntityId) -> Option<&T> {
        self.component_storage.get::<T>(entity)
    }

    pub fn get_component_mut<T: Component>(&mut self, entity: EntityId) -> Option<&mut T> {
        self.component_storage.get_mut::<T>(entity)
    }

    pub fn remove_component<T: Component>(&mut self, entity: EntityId) -> EverNightResult<()> {
        if !self.id_allocator.is_valid(entity) {
            return Err(EverNightError::InvalidEntityId(entity));
        }
        self.command_buffer.push(Command::RemoveComponent { entity, component_type_id: std::any::TypeId::of::<T>() });
        Ok(())
    }

    pub fn get_events(&self) -> &[EventPayload] {
        self.event_bus.events()
    }

    /// Returns entity IDs of all entities that have a component with the given `TypeId`.
    pub fn iter_entities_with_component(&self, type_id: std::any::TypeId) -> Vec<EntityId> {
        self.component_storage.iter_ids_dyn(type_id).collect()
    }

    /// Returns entity IDs of all entities whose `Tag` component has the given flags set.
    pub fn find_entities_with_tag(&self, flags: TagFlags) -> Vec<EntityId> {
        self.component_storage
            .iter::<Tag>()
            .filter(|(_, tag)| tag.has_flag(flags))
            .map(|(id, _)| id)
            .collect()
    }

    /// Gets a component reference by dynamic `TypeId` (committed state only).
    pub fn get_component_dyn(&self, entity: EntityId, type_id: std::any::TypeId) -> Option<&dyn Component> {
        self.component_storage.get_dyn(entity, type_id)
    }

    /// Queues an `AddComponent` command with a pre-boxed component.
    pub fn add_component_boxed(&mut self, entity: EntityId, component: Box<dyn Component>) -> EverNightResult<()> {
        if !self.id_allocator.is_valid(entity) {
            return Err(EverNightError::InvalidEntityId(entity));
        }
        self.command_buffer.push(Command::AddComponent { entity, component });
        Ok(())
    }

    /// Queues a `RemoveComponent` command by dynamic `TypeId`.
    pub fn remove_component_dyn(&mut self, entity: EntityId, type_id: std::any::TypeId) -> EverNightResult<()> {
        if !self.id_allocator.is_valid(entity) {
            return Err(EverNightError::InvalidEntityId(entity));
        }
        self.command_buffer.push(Command::RemoveComponent { entity, component_type_id: type_id });
        Ok(())
    }

    /// Returns `true` if the entity exists and has not been despawned.
    pub fn is_alive(&self, entity: EntityId) -> bool {
        self.id_allocator.is_valid(entity)
    }

    /// Iterates all entities that have component `T`, in ascending `EntityId` order.
    pub fn iter_components<T: Component>(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.component_storage.iter::<T>()
    }

    /// Mutably iterates all entities that have component `T`, in ascending `EntityId` order.
    pub fn iter_components_mut<T: Component>(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.component_storage.iter_mut::<T>()
    }

    // ── Low-level phase methods (used by App::step and World::step) ──────────

    /// Clears the event bus. Must be called at the start of each frame.
    pub fn clear_events_for_frame(&mut self) {
        self.event_bus.clear();
    }

    /// Drains and applies all buffered commands.
    ///
    /// `component_factory` is an optional callback used to instantiate components listed in a
    /// [`SpawnRequest`]. If `None` (or if the factory returns `None` for a given name), the
    /// component is silently skipped. Pass `Some(&registry.create_fn())` from the script layer.
    ///
    /// Emits `Spawned` / `Despawned` events as side-effects.
    pub fn commit_commands(
        &mut self,
        component_factory: Option<&dyn Fn(&str, &[u8]) -> Option<Box<dyn Component>>>,
        template_factory: Option<&dyn Fn(u32) -> Option<Vec<Box<dyn Component>>>>,
    ) -> EverNightResult<()> {
        let commands = self.command_buffer.drain();
        for command in commands {
            match command {
                Command::Spawn { entity, request } => {
                    // Named components from the request itself
                    if let Some(factory) = component_factory {
                        for (name, data) in request.components() {
                            if let Some(component) = factory(name, data) {
                                self.component_storage.insert_boxed(entity, component);
                            }
                        }
                    }
                    // Template components (applied after named, so they can be overridden)
                    if let Some(tf) = template_factory
                        && let Some(tid) = request.template_id()
                            && let Some(components) = tf(tid) {
                                for component in components {
                                    self.component_storage.insert_boxed(entity, component);
                                }
                            }
                    self.event_bus.push(EventPayload::Spawned { entity, tick: self.tick });
                }
                Command::Despawn(entity) => {
                    if !self.id_allocator.is_valid(entity) {
                        return Err(EverNightError::InvalidEntityId(entity));
                    }
                    self.component_storage.remove_all(entity);
                    self.id_allocator.deallocate(entity)?;
                    self.event_bus.push(EventPayload::Despawned { entity, tick: self.tick });
                }
                Command::AddComponent { entity, component } => {
                    self.component_storage.insert_boxed(entity, component);
                }
                Command::RemoveComponent { entity, component_type_id } => {
                    self.component_storage.remove_by_type_id(entity, component_type_id);
                }
            }
        }
        Ok(())
    }

    /// Runs the movement system (velocity → position/rotation integration).
    pub fn run_movement_system(&mut self) {
        movement_system(&mut self.component_storage, self.fixed_step.delta_time);
    }

    /// Runs the bounded system: clamps entities inside their declared region and
    /// reflects velocity on contact.  Must be called after `run_movement_system`.
    pub fn run_bounded_system(&mut self) {
        bounded_system(&mut self.component_storage);
    }

    /// Runs the collision system (broad-phase + narrow-phase, emits Collision events).
    pub fn run_collision_system(&mut self) {
        collision_system(&mut self.component_storage, &mut self.event_bus, self.tick);
    }

    /// Runs the elastic collision response system.  Applies velocity impulses to
    /// entity pairs that both carry `ElasticCollision`.  Must be called after
    /// `run_collision_system` so that collision events are already present.
    pub fn run_elastic_collision_system(&mut self) {
        use evernight_core::EventPayload;
        // Extract only what the system needs; avoids borrow conflicts between
        // `&mut component_storage` and `&event_bus`.
        let pairs: Vec<_> = self
            .event_bus
            .events()
            .iter()
            .filter_map(|e| {
                if let EventPayload::Collision { attacker, defender, normal, .. } = e {
                    Some((*attacker, *defender, *normal))
                } else {
                    None
                }
            })
            .collect();
        elastic_collision_system(&mut self.component_storage, &pairs);
    }

    /// Runs the lifetime system and immediately despawns expired entities.
    pub fn run_lifetime_system(&mut self) -> EverNightResult<()> {
        let expired = lifetime_system(&mut self.component_storage, &mut self.event_bus, self.tick);
        for entity in expired {
            self.component_storage.remove_all(entity);
            self.id_allocator.deallocate(entity)?;
        }
        Ok(())
    }

    /// Advances the tick counter and returns the step result.
    pub fn advance_tick(&mut self) -> StepResult {
        self.tick = Tick::new(self.tick.as_u32() + 1);
        StepResult {
            tick: self.tick,
            event_count: self.event_bus.events().len(),
        }
    }

    // ── High-level orchestrated step (for pure-runtime users) ─────────────────

    pub fn step(&mut self, scheduler: &mut Scheduler) -> EverNightResult<StepResult> {
        let dt = self.fixed_step.delta_time;
        let tick = self.tick;

        // 1. PreUpdate
        self.clear_events_for_frame();
        run_phase(&mut scheduler.pre_update, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);

        // 2. SpawnCommit
        self.commit_commands(None, None)?;
        run_phase(&mut scheduler.post_spawn_commit, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);

        // 3. Movement
        run_phase(&mut scheduler.pre_movement, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);
        self.run_movement_system();
        run_phase(&mut scheduler.post_movement, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);

        // 4. Collision
        run_phase(&mut scheduler.pre_collision, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);
        self.run_collision_system();
        run_phase(&mut scheduler.post_collision, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);

        // 5. Lifetime
        run_phase(&mut scheduler.pre_lifetime, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);
        self.run_lifetime_system()?;
        run_phase(&mut scheduler.post_lifetime, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);

        // 6. PostUpdate
        run_phase(&mut scheduler.post_update, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);

        // 7. Advance tick
        Ok(self.advance_tick())
    }

    /// Like [`step`](World::step) but also returns a per-phase timing breakdown.
    ///
    /// Only available when the `benchmark` feature is enabled.
    #[cfg(feature = "benchmark")]
    pub fn step_profiled(
        &mut self,
        scheduler: &mut Scheduler,
    ) -> EverNightResult<(StepResult, evernight_benchmark::WorldFrameProfile)> {
        use std::time::Instant;
        use evernight_benchmark::WorldFrameProfile;

        let mut p = WorldFrameProfile::default();
        let dt   = self.fixed_step.delta_time;
        let tick = self.tick;

        macro_rules! time {
            ($field:ident, $body:expr) => {{
                let _t = Instant::now();
                $body;
                p.$field = _t.elapsed();
            }};
        }

        time!(pre_update, {
            self.clear_events_for_frame();
            run_phase(&mut scheduler.pre_update, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt);
        });
        time!(spawn_commit,      self.commit_commands(None, None)?);
        time!(post_spawn_commit, run_phase(&mut scheduler.post_spawn_commit, &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(pre_movement,      run_phase(&mut scheduler.pre_movement,      &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(movement,          self.run_movement_system());
        time!(post_movement,     run_phase(&mut scheduler.post_movement,     &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(pre_collision,     run_phase(&mut scheduler.pre_collision,     &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(collision,         self.run_collision_system());
        time!(post_collision,    run_phase(&mut scheduler.post_collision,    &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(pre_lifetime,      run_phase(&mut scheduler.pre_lifetime,      &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(lifetime,          self.run_lifetime_system()?);
        time!(post_lifetime,     run_phase(&mut scheduler.post_lifetime,     &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));
        time!(post_update,       run_phase(&mut scheduler.post_update,       &mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, tick, dt));

        Ok((self.advance_tick(), p))
    }
}