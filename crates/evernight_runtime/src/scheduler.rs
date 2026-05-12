use evernight_core::Tick;

use crate::{CommandBuffer, ComponentStorage, EventBus};

/// A callable system that runs at a specific phase of the game loop.
///
/// Receives mutable access to component storage, the event bus, the command buffer,
/// the current tick, and the fixed delta time. Registration order within a phase
/// determines call order.
pub type SystemFn = Box<dyn FnMut(&mut ComponentStorage, &mut EventBus, &mut CommandBuffer, Tick, f32)>;

/// The phases of the fixed-step game loop, in execution order.
///
/// Hook lists in `Scheduler` mirror this ordering exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Before any commands are committed. Event bus has just been cleared.
    PreUpdate,
    /// After `Command`s are applied (spawns, despawns, component add/remove).
    PostSpawnCommit,
    /// After `movement_system` integrates velocity into position/rotation.
    PostMovement,
    /// After `collision_system` emits `Collision` events.
    PostCollision,
    /// After `lifetime_system` decrements counters and despawns expired entities.
    PostLifetime,
    /// Final phase before the tick counter is incremented.
    PostUpdate,
}

/// Holds user-registered systems called at specific phases during `World::step()`.
///
/// Systems within the same phase are called in registration order.
/// Registration order is deterministic as long as systems are registered in the same
/// order every run — which is enforced by fixed startup code.
#[derive(Default)]
pub struct Scheduler {
    pub(crate) pre_update: Vec<SystemFn>,
    pub(crate) post_spawn_commit: Vec<SystemFn>,
    pub(crate) post_movement: Vec<SystemFn>,
    pub(crate) post_collision: Vec<SystemFn>,
    pub(crate) post_lifetime: Vec<SystemFn>,
    pub(crate) post_update: Vec<SystemFn>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a system to run at the given `Phase`.
    pub fn add_system(&mut self, phase: Phase, system: SystemFn) {
        match phase {
            Phase::PreUpdate => self.pre_update.push(system),
            Phase::PostSpawnCommit => self.post_spawn_commit.push(system),
            Phase::PostMovement => self.post_movement.push(system),
            Phase::PostCollision => self.post_collision.push(system),
            Phase::PostLifetime => self.post_lifetime.push(system),
            Phase::PostUpdate => self.post_update.push(system),
        }
    }
}
