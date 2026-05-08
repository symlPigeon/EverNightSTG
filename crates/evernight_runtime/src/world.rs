use evernight_core::{IdAllocator, Tick};

use crate::ComponentStorage;

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
    
}
