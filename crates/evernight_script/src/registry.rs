use std::collections::HashMap;

use evernight_core::Component;

#[derive(Debug, Default)]
pub struct TagRegistry {
    name_to_id: HashMap<String, u32>,
    id_to_name: Vec<String>,
}

impl TagRegistry {
    pub fn new() -> Self {
        TagRegistry {
            name_to_id: HashMap::new(),
            id_to_name: Vec::new(),
        }
    }

    pub fn register(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(name) {
            id
        } else {
            let id = self.id_to_name.len() as u32;
            self.name_to_id.insert(name.to_string(), id);
            self.id_to_name.push(name.to_string());
            id
        }
    }

    pub fn id_of(&self, name: &str) -> Option<u32> {
        self.name_to_id.get(name).copied()
    }

    pub fn name_of(&self, id: u32) -> Option<&str> {
        self.id_to_name.get(id as usize).map(|s| s.as_str())
    }
}

#[derive(Default)]
pub struct ComponentRegistry {
    factories: HashMap<String, Box<dyn Fn(&[u8]) -> Box<dyn Component>>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        ComponentRegistry {
            factories: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: &str, factory: F)
    where
        F: Fn(&[u8]) -> Box<dyn Component> + 'static,
    {
        self.factories.insert(name.to_string(), Box::new(factory));
    }

    pub fn create(&self, name: &str, data: &[u8]) -> Option<Box<dyn Component>> {
        self.factories.get(name).map(|factory| factory(data))
    }

    pub fn register_names(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }

    pub fn registered_count(&self) -> usize {
        self.factories.len()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    pub fn clear(&mut self) {
        self.factories.clear();
    }

    pub fn registered_names(&self) -> impl Iterator<Item = &str> {
        self.factories.keys().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_registry() {
        let mut registry = TagRegistry::new();
        let id1 = registry.register("Player");
        let id2 = registry.register("Enemy");
        assert_eq!(registry.id_of("Player"), Some(id1));
        assert_eq!(registry.id_of("Enemy"), Some(id2));
        assert_eq!(registry.name_of(id1), Some("Player"));
        assert_eq!(registry.name_of(id2), Some("Enemy"));
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = TagRegistry::new();
        let id1 = registry.register("Player");
        let id2 = registry.register("Player");
        assert_eq!(id1, id2);
        assert_eq!(registry.id_of("Player"), Some(id1));
        assert_eq!(registry.name_of(id1), Some("Player"));
    }

    #[test]
    fn test_nonexistent_lookup() {
        let registry = TagRegistry::new();
        assert_eq!(registry.id_of("Nonexistent"), None);
        assert_eq!(registry.name_of(999), None);
    }

    // ── ComponentRegistry ─────────────────────────────────────────────────────

    // Minimal stub component used only inside this test module.
    use evernight_core::impl_component;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Marker(u32);
    impl_component!(Marker);

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Other(u32);
    impl_component!(Other);

    #[test]
    fn component_registry_create_registered() {
        let mut reg = ComponentRegistry::new();
        reg.register("Marker", |_| Box::new(Marker(42)));

        let component = reg.create("Marker", &[]);
        assert!(component.is_some());

        let marker = component.unwrap();
        let downcast = marker.as_any().downcast_ref::<Marker>();
        assert_eq!(downcast, Some(&Marker(42)));
    }

    #[test]
    fn component_registry_create_unregistered_returns_none() {
        let reg = ComponentRegistry::new();
        assert!(reg.create("Nonexistent", &[]).is_none());
    }

    #[test]
    fn component_registry_overwrite_replaces_factory() {
        let mut reg = ComponentRegistry::new();
        reg.register("Marker", |_| Box::new(Marker(1)));
        reg.register("Marker", |_| Box::new(Marker(99)));

        let marker = reg.create("Marker", &[]).unwrap();
        let downcast = marker.as_any().downcast_ref::<Marker>();
        assert_eq!(downcast, Some(&Marker(99)));
    }

    #[test]
    fn component_registry_registered_names_lists_all() {
        let mut reg = ComponentRegistry::new();
        reg.register("Marker", |_| Box::new(Marker(0)));
        reg.register("Other", |_| Box::new(Other(0)));

        let mut names: Vec<&str> = reg.registered_names().collect();
        names.sort_unstable();
        assert_eq!(names, vec!["Marker", "Other"]);
    }

    #[test]
    fn component_registry_is_registered() {
        let mut reg = ComponentRegistry::new();
        assert!(!reg.is_registered("Marker"));
        reg.register("Marker", |_| Box::new(Marker(0)));
        assert!(reg.is_registered("Marker"));
    }

    #[test]
    fn component_registry_clear_removes_all() {
        let mut reg = ComponentRegistry::new();
        reg.register("Marker", |_| Box::new(Marker(0)));
        reg.clear();
        assert_eq!(reg.registered_count(), 0);
        assert!(reg.create("Marker", &[]).is_none());
    }

    #[test]
    fn component_registry_factory_called_per_create() {
        // Each call to create() should return a fresh, independent instance.
        let mut reg = ComponentRegistry::new();
        reg.register("Marker", |_| Box::new(Marker(7)));

        let a = reg.create("Marker", &[]).unwrap();
        let b = reg.create("Marker", &[]).unwrap();
        assert_eq!(a.as_any().downcast_ref::<Marker>(), Some(&Marker(7)));
        assert_eq!(b.as_any().downcast_ref::<Marker>(), Some(&Marker(7)));
    }
}
