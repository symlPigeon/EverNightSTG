use evernight_core::Tick;

use crate::{CommandBuffer, ComponentStorage, EventBus};

/// Execution priority for built-in engine behaviors.
/// Lower value → runs earlier within a phase.
pub const PRIORITY_BUILTIN: i32 = 0;

/// Default execution priority for user-registered systems.
/// Runs after [`PRIORITY_BUILTIN`] systems.
pub const PRIORITY_DEFAULT: i32 = 100;

/// A callable system that runs at a specific phase of the game loop.
///
/// Receives mutable access to component storage, the event bus, the command buffer,
/// the current tick, and the fixed delta time.
pub type SystemFn =
    Box<dyn FnMut(&mut ComponentStorage, &mut EventBus, &mut CommandBuffer, Tick, f32)>;

/// A system paired with its execution priority.
///
/// Within a phase, entries are sorted by `priority` ascending; equal-priority entries
/// run in registration order (FIFO).
pub struct SystemEntry {
    /// Execution priority. Lower value → runs earlier.
    /// Use [`PRIORITY_BUILTIN`] for engine behaviors and [`PRIORITY_DEFAULT`] for user systems.
    pub priority: i32,
    pub(crate) system: SystemFn,
}

/// The phases of the fixed-step game loop, in execution order.
///
/// Each core system is bracketed by a `Pre` phase (intent: *prepare for* the system)
/// and a `Post` phase (intent: *react to* what the system just did).  `PreX` and the
/// preceding `PostY` share the same wall-clock moment but carry different semantic
/// ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Start of tick; also serves as the implicit *pre-spawn-commit* phase.
    /// Queue spawn / despawn / mutation [`Command`]s here — they are applied
    /// immediately after all `PreUpdate` systems finish.
    ///
    /// [`Command`]: crate::Command
    PreUpdate,
    /// After `Command`s are applied (spawns, despawns, component mutations).
    PostSpawnCommit,
    /// Just before `movement_system` runs. Intent: reposition / pre-integrate.
    PreMovement,
    /// After `movement_system` integrates velocity → position. Intent: react to new positions.
    PostMovement,
    /// Just before `collision_system` runs. Intent: adjust hitboxes or cull pairs.
    PreCollision,
    /// After `collision_system` emits `Collision` events. Intent: respond to overlaps.
    PostCollision,
    /// Just before `lifetime_system` runs. Intent: queue late despawns.
    PreLifetime,
    /// After `lifetime_system` decrements counters and despawns expired entities.
    PostLifetime,
    /// Final phase before the tick counter is incremented.
    PostUpdate,
}

/// Holds systems called at specific phases during [`World::step()`].
///
/// Each phase stores a priority-sorted list of [`SystemEntry`] values.  Lower
/// `priority` → runs earlier; equal-priority systems run in registration order (FIFO).
///
/// Use [`PRIORITY_BUILTIN`] for component-driven engine behaviors (e.g. `bounded_system`)
/// and [`PRIORITY_DEFAULT`] for general user systems.
#[derive(Default)]
pub struct Scheduler {
    pub(crate) pre_update: Vec<SystemEntry>,
    pub(crate) post_spawn_commit: Vec<SystemEntry>,
    pub(crate) pre_movement: Vec<SystemEntry>,
    pub(crate) post_movement: Vec<SystemEntry>,
    pub(crate) pre_collision: Vec<SystemEntry>,
    pub(crate) post_collision: Vec<SystemEntry>,
    pub(crate) pre_lifetime: Vec<SystemEntry>,
    pub(crate) post_lifetime: Vec<SystemEntry>,
    pub(crate) post_update: Vec<SystemEntry>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a system to run at the given `phase` with the specified `priority`.
    ///
    /// Lower priority value → runs earlier.  Equal-priority systems run in
    /// registration order (FIFO).
    pub fn add_system(&mut self, phase: Phase, priority: i32, system: SystemFn) {
        let entries = self.entries_mut(phase);
        let pos = entries.partition_point(|e| e.priority <= priority);
        entries.insert(pos, SystemEntry { priority, system });
    }

    fn entries_mut(&mut self, phase: Phase) -> &mut Vec<SystemEntry> {
        match phase {
            Phase::PreUpdate => &mut self.pre_update,
            Phase::PostSpawnCommit => &mut self.post_spawn_commit,
            Phase::PreMovement => &mut self.pre_movement,
            Phase::PostMovement => &mut self.post_movement,
            Phase::PreCollision => &mut self.pre_collision,
            Phase::PostCollision => &mut self.post_collision,
            Phase::PreLifetime => &mut self.pre_lifetime,
            Phase::PostLifetime => &mut self.post_lifetime,
            Phase::PostUpdate => &mut self.post_update,
        }
    }
}
