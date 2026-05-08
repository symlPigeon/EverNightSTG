use evernight_math::Vec2;

use crate::{EntityId, Tick};

/// Protocol for runtime events.
/// Events carry metadata about the running simulation.
pub trait EventLike {
    /// The primary entity involved in this event.
    fn primary_entity(&self) -> Option<EntityId>;

    /// The secondary entity involved, if any (e.g., defender in a Collision).
    fn secondary_entity(&self) -> Option<EntityId>;

    /// The simulation tick when this event occurred (None only for Custom).
    fn occurred_tick(&self) -> Option<Tick>;
}

pub enum EventPayload {
    /// Collision between two entities (attacker's Hitbox vs defender's Hurtbox).
    Collision {
        attacker: EntityId,
        defender: EntityId,
        contact_point: Vec2,
        normal: Vec2,
        tick: Tick,
    },
    /// An entity was spawned.
    Spawned { entity: EntityId, tick: Tick },
    /// An entity was despawned.
    Despawned { entity: EntityId, tick: Tick },
    /// An entity's lifetime expired.
    LifetimeExpired { entity: EntityId, tick: Tick },
    /// Custom user-defined event.
    Custom { name: String, data: Vec<u8> },
}

impl EventLike for EventPayload {
    fn primary_entity(&self) -> Option<EntityId> {
        match self {
            EventPayload::Collision { attacker, .. } => Some(*attacker),
            EventPayload::Spawned { entity, .. } => Some(*entity),
            EventPayload::Despawned { entity, .. } => Some(*entity),
            EventPayload::LifetimeExpired { entity, .. } => Some(*entity),
            EventPayload::Custom { .. } => None,
        }
    }

    fn secondary_entity(&self) -> Option<EntityId> {
        match self {
            EventPayload::Collision { defender, .. } => Some(*defender),
            _ => None,
        }
    }

    fn occurred_tick(&self) -> Option<Tick> {
        match self {
            EventPayload::Collision { tick, .. } => Some(*tick),
            EventPayload::Spawned { tick, .. } => Some(*tick),
            EventPayload::Despawned { tick, .. } => Some(*tick),
            EventPayload::LifetimeExpired { tick, .. } => Some(*tick),
            EventPayload::Custom { .. } => None,
        }
    }
}
