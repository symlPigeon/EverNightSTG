use std::any::TypeId;

use evernight_core::{Component, EntityId, SpawnRequest};

pub enum Command {
    Spawn {
        entity: EntityId,
        request: SpawnRequest,
    },
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

#[derive(Default)]
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

    /// Drains all buffered commands and returns them, leaving the buffer empty.
    pub fn drain(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}
