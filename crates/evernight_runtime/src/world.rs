use evernight_core::{Component, EntityId, EventPayload, EverNightError, EverNightResult, IdAllocator, SpawnRequest, Tick};

use crate::{Command, CommandBuffer, ComponentStorage, EventBus, Scheduler, collision_system, lifetime_system, movement_system};

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

    pub fn step(&mut self, scheduler: &mut Scheduler) -> EverNightResult<StepResult> {
        // 1. PreUpdate: clear the previous frame's events, run pre-update hooks.
        self.event_bus.clear();
        for hook in &mut scheduler.pre_update {
            hook(&mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, self.tick, self.fixed_step.delta_time);
        }

        // 2. SpawnCommit: drain and apply all buffered commands.
        let commands = self.command_buffer.drain();
        for command in commands {
            match command {
                Command::Spawn { entity, request } => {
                    // Entity ID was pre-allocated in spawn_entity().
                    // TODO: template instantiation via component registry once script layer is ready.
                    let _ = request;
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
        for hook in &mut scheduler.post_spawn_commit {
            hook(&mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, self.tick, self.fixed_step.delta_time);
        }

        // 3. Movement: integrate velocity into position/rotation.
        movement_system(&mut self.component_storage, self.fixed_step.delta_time);
        for hook in &mut scheduler.post_movement {
            hook(&mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, self.tick, self.fixed_step.delta_time);
        }

        // 4. Collision: detect overlaps and emit Collision events.
        collision_system(&mut self.component_storage, &mut self.event_bus, self.tick);
        for hook in &mut scheduler.post_collision {
            hook(&mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, self.tick, self.fixed_step.delta_time);
        }

        // 5. Lifetime: decrement counters; immediately despawn expired entities.
        let expired = lifetime_system(&mut self.component_storage, &mut self.event_bus, self.tick);
        for entity in expired {
            self.component_storage.remove_all(entity);
            self.id_allocator.deallocate(entity)?;
        }
        for hook in &mut scheduler.post_lifetime {
            hook(&mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, self.tick, self.fixed_step.delta_time);
        }

        // 6. PostUpdate: final user hooks before tick is incremented.
        for hook in &mut scheduler.post_update {
            hook(&mut self.component_storage, &mut self.event_bus, &mut self.command_buffer, self.tick, self.fixed_step.delta_time);
        }

        // 7. Advance tick.
        self.tick = Tick::new(self.tick.as_u32() + 1);

        Ok(StepResult {
            tick: self.tick,
            event_count: self.event_bus.events().len(),
        })
    }
}