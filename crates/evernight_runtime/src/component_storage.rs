use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap},
};

use evernight_core::{Component, EntityId};

/// lightweight ECS
#[derive(Default)]
pub struct ComponentStorage {
    columns: HashMap<TypeId, BTreeMap<EntityId, Box<dyn Component>>>,
}

impl ComponentStorage {
    pub fn new() -> Self {
        ComponentStorage {
            columns: HashMap::new(),
        }
    }

    pub fn insert<T: Component>(&mut self, entity: EntityId, component: T) {
        let type_id = TypeId::of::<T>();
        let column = self.columns.entry(type_id).or_default();
        column.insert(entity, Box::new(component));
    }

    pub fn get<T: Component>(&self, entity: EntityId) -> Option<&T> {
        let type_id = TypeId::of::<T>();
        self.columns
            .get(&type_id)
            .and_then(|column| column.get(&entity))
            .and_then(|boxed| boxed.as_any().downcast_ref::<T>())
    }

    pub fn get_mut<T: Component>(&mut self, entity: EntityId) -> Option<&mut T> {
        let type_id = TypeId::of::<T>();
        self.columns
            .get_mut(&type_id)
            .and_then(|column| column.get_mut(&entity))
            .and_then(|boxed| boxed.as_any_mut().downcast_mut::<T>())
    }

    pub fn remove<T: Component>(&mut self, entity: EntityId) -> Option<T> {
        let type_id = TypeId::of::<T>();
        let boxed = self.columns.get_mut(&type_id)?.remove(&entity)?;
        // Safety: TypeId guarantees the stored value is exactly T
        let raw = Box::into_raw(boxed);
        let typed = unsafe { Box::from_raw(raw as *mut T) };
        Some(*typed)
    }

    pub fn remove_all(&mut self, entity: EntityId) {
        for column in self.columns.values_mut() {
            column.remove(&entity);
        }
    }

    pub fn iter<T: Component>(&self) -> impl Iterator<Item = (EntityId, &T)> {
        let type_id = TypeId::of::<T>();
        self.columns
            .get(&type_id)
            .into_iter()
            .flat_map(|column| column.iter())
            .map(|(entity, boxed)| {
                let component = boxed.as_any().downcast_ref::<T>().unwrap();
                (*entity, component)
            })
    }

    pub fn iter_mut<T: Component>(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        let type_id = TypeId::of::<T>();
        self.columns
            .get_mut(&type_id)
            .into_iter()
            .flat_map(|column| column.iter_mut())
            .map(|(entity, boxed)| {
                let component = boxed.as_any_mut().downcast_mut::<T>().unwrap();
                (*entity, component)
            })
    }

    /// Inserts a dynamically-typed component. The `TypeId` is derived from `as_any().type_id()`.
    /// Used by `CommandBuffer` when applying `Command::AddComponent`.
    pub fn insert_boxed(&mut self, entity: EntityId, component: Box<dyn Component>) {
        let type_id = (*component).as_any().type_id();
        self.columns
            .entry(type_id)
            .or_default()
            .insert(entity, component);
    }

    /// Removes a component by `TypeId` without needing the concrete type.
    /// Used by `CommandBuffer` when applying `Command::RemoveComponent`.
    pub fn remove_by_type_id(&mut self, entity: EntityId, type_id: TypeId) {
        if let Some(column) = self.columns.get_mut(&type_id) {
            column.remove(&entity);
        }
    }

    /// Gets a component reference by dynamic `TypeId`, without knowing the concrete type.
    pub fn get_dyn(&self, entity: EntityId, type_id: TypeId) -> Option<&dyn Component> {
        self.columns
            .get(&type_id)?
            .get(&entity)
            .map(|b| b.as_ref())
    }
}
