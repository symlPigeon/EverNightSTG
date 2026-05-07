use crate::EntityId;

/// Request payload used to spawn an entity at the command-commit stage.
///
/// This type is a data carrier collected by the runtime command buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnRequest {
    template_id: Option<u32>,
    components: Vec<(String, Vec<u8>)>,
}

impl SpawnRequest {
    /// Creates an empty spawn request.
    pub fn new() -> Self {
        SpawnRequest {
            template_id: None,
            components: Vec::new(),
        }
    }

    /// Creates a spawn request with a preselected template id.
    pub fn with_template(template_id: u32) -> Self {
        SpawnRequest {
            template_id: Some(template_id),
            components: Vec::new(),
        }
    }

    /// Adds one serialized component payload to the spawn request.
    ///
    /// Builder-style API: returns the updated request.
    pub fn add_component(mut self, name: &str, data: Vec<u8>) -> Self {
        self.components.push((name.to_string(), data));
        self
    }

    /// Returns the optional template id associated with this request.
    pub fn template_id(&self) -> Option<u32> {
        self.template_id
    }

    /// Returns all component payloads as a shared slice.
    pub fn components(&self) -> &[(String, Vec<u8>)] {
        &self.components
    }

    /// Returns an iterator over component payloads.
    pub fn components_iter(&self) -> impl Iterator<Item = &(String, Vec<u8>)> {
        self.components.iter()
    }
}

impl Default for SpawnRequest {
    /// Equivalent to [`SpawnRequest::new`].
    fn default() -> Self {
        Self::new()
    }
}

/// Request payload used to despawn an existing entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DespawnRequest {
    entity: EntityId,
}

impl DespawnRequest {
    /// Creates a despawn request for a specific entity.
    pub fn new(entity: EntityId) -> Self {
        DespawnRequest { entity }
    }

    /// Returns the entity to despawn.
    pub fn entity(&self) -> EntityId {
        self.entity
    }
}
