use std::any::TypeId;

use evernight_core::{Component, EntityId, EverNightResult, SpawnRequest};

use crate::World;

pub enum Command {
    Spawn(SpawnRequest),
    Despawn(EntityId),
    AddComponent {
        entity: EntityId,
        component: Box<dyn Component>,
    },
    RemoveComponent {
        entity: EntityId,
        component_type_id: TypeId,
    },
}

pub struct CommandBuffer {
    commands: Vec<Command>,
}

impl CommandBuffer {
    pub fn new() -> Self {
        CommandBuffer {
            commands: Vec::new(),
        }
    }

    pub fn push(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn apply(&mut self, world: &mut World) -> EverNightResult<()> {
        Ok(())
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}
