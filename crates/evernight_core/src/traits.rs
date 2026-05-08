use crate::{EventPayload, EverNightResult, SpawnRequest};

/// A trait for types that can be used as components in the ECS.
pub trait Component: Send + Sync + 'static {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// Spawnable entities
pub trait Spawnable: Send + Sync + 'static {
    fn create_entity() -> SpawnRequest;
}

/// Event Consumer
pub trait EventConsumer: Send + Sync + 'static {
    fn on_event(&mut self, payload: &EventPayload) -> EverNightResult<()>;
}