use std::{any::TypeId, collections::HashMap};

use evernight_core::Component;
use mlua::{Lua, Table};

pub(crate) struct LuaComponentEntry {
    pub(crate) type_id: TypeId,
    pub(crate) to_table: Box<dyn Fn(&dyn Component, &Lua) -> mlua::Result<Table>>,
    pub(crate) from_table: Box<dyn Fn(&Table) -> mlua::Result<Box<dyn Component>>>,
}

/// Registry that maps component names to Lua serialization/deserialization callbacks.
///
/// Register each component type you want to expose to Lua scripts using
/// [`LuaComponentRegistry::register`], then pass them to
/// [`LuaEngine::register_component`].
///
/// # Example
/// ```rust,ignore
/// engine.register_component::<Position>(
///     "Position",
///     |p, lua| {
///         let t = lua.create_table()?;
///         t.set("x", p.x)?;
///         t.set("y", p.y)?;
///         Ok(t)
///     },
///     |t| Ok(Position { x: t.get("x")?, y: t.get("y")? }),
/// );
/// ```
pub struct LuaComponentRegistry {
    entries: HashMap<String, LuaComponentEntry>,
}

impl LuaComponentRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Registers a component type with serialize (`to_table`) and deserialize
    /// (`from_table`) callbacks.
    ///
    /// - `to_table`: converts a `&T` plus `&Lua` into a Lua table (for reading).
    /// - `from_table`: converts a Lua `&Table` into a `T` (for writing).
    pub fn register<T, FTo, FFrom>(&mut self, name: &str, to_table: FTo, from_table: FFrom)
    where
        T: Component,
        FTo: Fn(&T, &Lua) -> mlua::Result<Table> + 'static,
        FFrom: Fn(&Table) -> mlua::Result<T> + 'static,
    {
        let to: Box<dyn Fn(&dyn Component, &Lua) -> mlua::Result<Table>> =
            Box::new(move |comp, lua| {
                let concrete = comp
                    .as_any()
                    .downcast_ref::<T>()
                    .expect("component type mismatch in to_table");
                to_table(concrete, lua)
            });
        let from: Box<dyn Fn(&Table) -> mlua::Result<Box<dyn Component>>> =
            Box::new(move |t| from_table(t).map(|c| Box::new(c) as Box<dyn Component>));
        self.entries.insert(
            name.to_string(),
            LuaComponentEntry {
                type_id: TypeId::of::<T>(),
                to_table: to,
                from_table: from,
            },
        );
    }

    /// Returns the entry for a component name, or `None` if not registered.
    pub(crate) fn get_entry(&self, name: &str) -> Option<&LuaComponentEntry> {
        self.entries.get(name)
    }

    /// Deserializes a Lua table into a boxed component using the registered callback.
    /// Returns `None` if the component name is not registered.
    pub(crate) fn call_from_table(
        &self,
        name: &str,
        table: &Table,
    ) -> Option<mlua::Result<Box<dyn Component>>> {
        self.entries.get(name).map(|e| (e.from_table)(table))
    }

    /// Returns `true` if `name` has been registered.
    pub fn is_registered(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }
}

impl Default for LuaComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
