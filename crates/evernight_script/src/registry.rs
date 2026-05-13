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

// ── TemplateRegistry ──────────────────────────────────────────────────────────

/// A no-argument factory that produces one component instance.
pub type TemplateComponentFn = Box<dyn Fn() -> Box<dyn Component>>;

/// Maps template names to ordered lists of component factories.
///
/// A *template* is a reusable recipe for spawning a fully-equipped entity.
/// Each factory in the list is called once per spawn to produce an independent
/// component instance.  Templates are identified at runtime by a stable `u32` ID.
///
/// # Example
/// ```rust,ignore
/// let bullet_id = template_registry.register("player_bullet", vec![
///     Box::new(|| Box::new(Transform::identity())),
///     Box::new(|| Box::new(Velocity::zero())),
/// ]);
/// // Later: SpawnRequest::with_template(bullet_id)
/// ```
#[derive(Default)]
pub struct TemplateRegistry {
    name_to_id: HashMap<String, u32>,
    templates: Vec<Vec<TemplateComponentFn>>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a template and returns its stable `u32` ID.
    ///
    /// Re-registering the same name replaces the existing template but keeps the ID.
    pub fn register(&mut self, name: &str, components: Vec<TemplateComponentFn>) -> u32 {
        if let Some(&id) = self.name_to_id.get(name) {
            self.templates[id as usize] = components;
            id
        } else {
            let id = self.templates.len() as u32;
            self.name_to_id.insert(name.to_string(), id);
            self.templates.push(components);
            id
        }
    }

    /// Looks up the ID for a template name.
    pub fn id_of(&self, name: &str) -> Option<u32> {
        self.name_to_id.get(name).copied()
    }

    /// Instantiates all components for the given template ID.
    ///
    /// Returns `None` if the ID is out of range.  Each call produces fresh instances.
    pub fn instantiate(&self, id: u32) -> Option<Vec<Box<dyn Component>>> {
        self.templates.get(id as usize).map(|factories| {
            factories.iter().map(|f| f()).collect()
        })
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.name_to_id.contains_key(name)
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

    // ── TemplateRegistry ──────────────────────────────────────────────────────

    #[test]
    fn template_registry_register_and_instantiate() {
        let mut reg = TemplateRegistry::new();
        let id = reg.register(
            "bullet",
            vec![
                Box::new(|| Box::new(Marker(1)) as Box<dyn evernight_core::Component>),
                Box::new(|| Box::new(Other(2)) as Box<dyn evernight_core::Component>),
            ],
        );
        let components = reg.instantiate(id).expect("should return components");
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].as_any().downcast_ref::<Marker>(), Some(&Marker(1)));
        assert_eq!(components[1].as_any().downcast_ref::<Other>(), Some(&Other(2)));
    }

    #[test]
    fn template_registry_id_of_returns_stable_id() {
        let mut reg = TemplateRegistry::new();
        let id = reg.register("bullet", vec![]);
        assert_eq!(reg.id_of("bullet"), Some(id));
        assert_eq!(reg.id_of("missing"), None);
    }

    #[test]
    fn template_registry_re_register_keeps_id() {
        let mut reg = TemplateRegistry::new();
        let id1 = reg.register("bullet", vec![
            Box::new(|| Box::new(Marker(1)) as Box<dyn evernight_core::Component>),
        ]);
        let id2 = reg.register("bullet", vec![
            Box::new(|| Box::new(Marker(99)) as Box<dyn evernight_core::Component>),
        ]);
        assert_eq!(id1, id2, "re-registering same name must reuse the same id");
        let components = reg.instantiate(id1).unwrap();
        assert_eq!(components[0].as_any().downcast_ref::<Marker>(), Some(&Marker(99)));
    }

    #[test]
    fn template_registry_unknown_id_returns_none() {
        let reg = TemplateRegistry::new();
        assert!(reg.instantiate(999).is_none());
    }

    #[test]
    fn template_registry_each_instantiate_produces_fresh_instances() {
        let mut reg = TemplateRegistry::new();
        let id = reg.register("bullet", vec![
            Box::new(|| Box::new(Marker(42)) as Box<dyn evernight_core::Component>),
        ]);
        let a = reg.instantiate(id).unwrap();
        let b = reg.instantiate(id).unwrap();
        // Both should be equal in value, proving independent construction
        assert_eq!(a[0].as_any().downcast_ref::<Marker>(), Some(&Marker(42)));
        assert_eq!(b[0].as_any().downcast_ref::<Marker>(), Some(&Marker(42)));
    }

    #[test]
    fn template_registry_is_registered() {
        let mut reg = TemplateRegistry::new();
        assert!(!reg.is_registered("bullet"));
        reg.register("bullet", vec![]);
        assert!(reg.is_registered("bullet"));
    }
}
