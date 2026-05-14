use evernight_core::{Component, EntityId, EventPayload, EverNightResult, SpawnRequest, Tick};
use evernight_runtime::World;

use crate::{ComponentRegistry, TagRegistry};

/// A restricted view of the game world passed to user systems registered via [`App`].
///
/// `ScriptContext` exposes the full entity/component API of [`World`] while also
/// providing access to the [`ComponentRegistry`] and [`TagRegistry`] owned by the
/// [`App`].  It intentionally does **not** expose `step()` or the internal scheduler
/// so that user hooks cannot trigger re-entrant frame updates.
pub struct ScriptContext<'a> {
    pub(crate) world: &'a mut World,
    pub(crate) component_registry: &'a ComponentRegistry,
    pub(crate) tag_registry: &'a TagRegistry,
}

impl<'a> ScriptContext<'a> {
    pub(crate) fn new(
        world: &'a mut World,
        component_registry: &'a ComponentRegistry,
        tag_registry: &'a TagRegistry,
    ) -> Self {
        ScriptContext {
            world,
            component_registry,
            tag_registry,
        }
    }

    // ── Entity management ─────────────────────────────────────────────────────

    /// Pre-allocates an entity ID and queues a `Spawn` command.
    /// The entity becomes visible after the next `SpawnCommit` phase.
    pub fn spawn(&mut self, request: SpawnRequest) -> EverNightResult<EntityId> {
        self.world.spawn_entity(request)
    }

    /// Queues a `Despawn` command. The entity is removed after the next `SpawnCommit` phase.
    pub fn despawn(&mut self, entity: EntityId) -> EverNightResult<()> {
        self.world.despawn_entity(entity)
    }

    /// Returns `true` if the entity exists and has not been despawned.
    pub fn is_alive(&self, entity: EntityId) -> bool {
        self.world.is_alive(entity)
    }

    // ── Component access ──────────────────────────────────────────────────────

    /// Queues an `AddComponent` command.
    pub fn add_component<T: Component>(
        &mut self,
        entity: EntityId,
        component: T,
    ) -> EverNightResult<()> {
        self.world.add_component(entity, component)
    }

    /// Returns a reference to the component if the entity has it (reads current committed state).
    pub fn get_component<T: Component>(&self, entity: EntityId) -> Option<&T> {
        self.world.get_component::<T>(entity)
    }

    /// Returns a mutable reference to the component if the entity has it.
    pub fn get_component_mut<T: Component>(&mut self, entity: EntityId) -> Option<&mut T> {
        self.world.get_component_mut::<T>(entity)
    }

    /// Queues a `RemoveComponent` command.
    pub fn remove_component<T: Component>(&mut self, entity: EntityId) -> EverNightResult<()> {
        self.world.remove_component::<T>(entity)
    }

    /// Gets a component reference by dynamic `TypeId` (committed state only).
    pub fn get_component_dyn(
        &self,
        entity: EntityId,
        type_id: std::any::TypeId,
    ) -> Option<&dyn Component> {
        self.world.get_component_dyn(entity, type_id)
    }

    /// Queues an `AddComponent` command with a pre-boxed component.
    pub fn add_component_boxed(
        &mut self,
        entity: EntityId,
        component: Box<dyn Component>,
    ) -> EverNightResult<()> {
        self.world.add_component_boxed(entity, component)
    }

    /// Queues a `RemoveComponent` command by dynamic `TypeId`.
    pub fn remove_component_dyn(
        &mut self,
        entity: EntityId,
        type_id: std::any::TypeId,
    ) -> EverNightResult<()> {
        self.world.remove_component_dyn(entity, type_id)
    }

    // ── Events ────────────────────────────────────────────────────────────────

    /// Returns all events emitted so far this frame.
    pub fn events(&self) -> &[EventPayload] {
        self.world.get_events()
    }

    /// Returns entity IDs of all entities that have a component with the given `TypeId`.
    pub fn iter_entities_with_component(&self, type_id: std::any::TypeId) -> Vec<EntityId> {
        self.world.iter_entities_with_component(type_id)
    }

    /// Returns entity IDs of all entities whose `Tag` component has the given flags set.
    pub fn find_entities_with_tag(&self, flags: evernight_core::TagFlags) -> Vec<EntityId> {
        self.world.find_entities_with_tag(flags)
    }

    // ── Registries ────────────────────────────────────────────────────────────

    pub fn component_registry(&self) -> &ComponentRegistry {
        self.component_registry
    }

    pub fn tag_registry(&self) -> &TagRegistry {
        self.tag_registry
    }

    // ── Time ─────────────────────────────────────────────────────────────────

    pub fn tick(&self) -> Tick {
        self.world.tick()
    }

    pub fn delta_time(&self) -> f32 {
        self.world.delta_time()
    }
}
