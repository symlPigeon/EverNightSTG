use crate::types::{EntityId, EverNightError, EverNightResult};

/// Allocates and recycles `EntityId` within a single game world.
///
/// Each `World` instance owns its own allocator.
/// IDs are recycled when entities are despawned.
#[derive(Debug)]
pub struct IdAllocator {
    next_id: u32,
    free_ids: Vec<EntityId>,
    valid_ids: std::collections::HashSet<EntityId>,
}

impl IdAllocator {
    /// Creates a new allocator. Starts ID allocation from 1 (0 is reserved).
    pub fn new() -> Self {
        IdAllocator {
            next_id: 1,
            free_ids: Vec::new(),
            valid_ids: std::collections::HashSet::new(),
        }
    }

    /// Allocates a new `EntityId`.
    ///
    /// Reuses recycled IDs or generates new ones sequentially.
    /// Returns an error if the allocator is exhausted (ID counter reached u32::MAX).
    pub fn allocate(&mut self) -> EverNightResult<EntityId> {
        let id = if let Some(free_id) = self.free_ids.pop() {
            free_id
        } else {
            if self.next_id == u32::MAX {
                return Err(EverNightError::AllocatorFull);
            }
            let id_val = self.next_id;
            self.next_id += 1;
            EntityId::new(id_val)
        };
        self.valid_ids.insert(id);
        Ok(id)
    }

    /// Deallocates an `EntityId` for reuse.
    ///
    /// Panics in debug mode if the ID is invalid.
    /// Returns an error if already deallocated.
    pub fn deallocate(&mut self, id: EntityId) -> EverNightResult<()> {
        if !self.valid_ids.remove(&id) {
            return Err(EverNightError::InvalidEntityId(id));
        }
        self.free_ids.push(id);
        Ok(())
    }

    /// Checks whether an `EntityId` is currently valid.
    pub fn is_valid(&self, id: EntityId) -> bool {
        self.valid_ids.contains(&id)
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}
