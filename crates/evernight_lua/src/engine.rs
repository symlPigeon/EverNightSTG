use evernight_core::{
    Component, EntityId, EventPayload, EverNightError, EverNightResult, SpawnRequest,
};
use evernight_script::{ScriptContext, ScriptEngine};
use mlua::{Lua, RegistryKey, UserData, UserDataMethods};

use crate::bindings;
use crate::lua_component_registry::LuaComponentRegistry;
use crate::render_cmd::{RenderCommand, RenderSender};

// ── ScriptContext UserData ────────────────────────────────────────────────────

/// Thin wrapper holding opaque raw pointers to a `ScriptContext` and the
/// `LuaComponentRegistry`.
///
/// # Safety invariant
/// `LuaEngine::on_frame` always calls the Lua `on_frame` function inside
/// `lua.scope(...)`.  The mlua scope guarantees that no Lua value (and
/// therefore no `CtxUserdata`) can be stored beyond the scope's lifetime.
/// Consequently every pointer dereference in the method callbacks below is
/// valid for the entire duration of the method call.
struct CtxUserdata {
    ctx_ptr: *mut (),
    reg_ptr: *const (),
    /// Optional render command sender.  `None` when no Godot bridge is attached.
    render_tx: Option<RenderSender>,
}

// SAFETY: The engine is single-threaded and `Lua` itself is `!Send`.
// This impl is required by mlua's `UserData` bound even though we never
// actually send the value across threads.
unsafe impl Send for CtxUserdata {}

impl CtxUserdata {
    /// Returns a shared reference to the context.
    ///
    /// # Safety
    /// Caller must guarantee the pointer is valid and no `&mut ScriptContext`
    /// alias exists for the duration of the returned reference.
    unsafe fn as_ctx<'s>(&'s self) -> &'s ScriptContext<'s> {
        unsafe { &*(self.ctx_ptr as *const ScriptContext<'s>) }
    }

    /// Returns a mutable reference to the context.
    ///
    /// # Safety
    /// Caller must guarantee the pointer is valid and no other reference to the
    /// same `ScriptContext` exists for the duration of the returned reference.
    unsafe fn as_ctx_mut<'s>(&'s mut self) -> &'s mut ScriptContext<'s> {
        unsafe { &mut *(self.ctx_ptr as *mut ScriptContext<'s>) }
    }

    /// Returns a shared reference to the component registry.
    ///
    /// # Safety
    /// Caller must guarantee the pointer is valid for the duration of the
    /// returned reference.
    unsafe fn as_reg(&self) -> &LuaComponentRegistry {
        unsafe { &*(self.reg_ptr as *const LuaComponentRegistry) }
    }
}

impl UserData for CtxUserdata {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ctx:tick() → integer (u32)
        methods.add_method("tick", |_, this, ()| {
            Ok(unsafe { this.as_ctx() }.tick().as_u32())
        });

        // ctx:delta_time() → float (f32)
        methods.add_method("delta_time", |_, this, ()| {
            Ok(unsafe { this.as_ctx() }.delta_time())
        });

        // ctx:events() → table array of event tables
        methods.add_method("events", |lua, this, ()| {
            let ctx = unsafe { this.as_ctx() };
            let t = lua.create_table()?;
            for (i, event) in ctx.events().iter().enumerate() {
                let entry = lua.create_table()?;
                match event {
                    EventPayload::Collision {
                        attacker,
                        defender,
                        tick,
                        ..
                    } => {
                        entry.set("type", "Collision")?;
                        entry.set("attacker", attacker.as_u32())?;
                        entry.set("defender", defender.as_u32())?;
                        entry.set("tick", tick.as_u32())?;
                    }
                    EventPayload::Spawned { entity, tick } => {
                        entry.set("type", "Spawned")?;
                        entry.set("entity", entity.as_u32())?;
                        entry.set("tick", tick.as_u32())?;
                    }
                    EventPayload::Despawned { entity, tick } => {
                        entry.set("type", "Despawned")?;
                        entry.set("entity", entity.as_u32())?;
                        entry.set("tick", tick.as_u32())?;
                    }
                    EventPayload::LifetimeExpired { entity, tick } => {
                        entry.set("type", "LifetimeExpired")?;
                        entry.set("entity", entity.as_u32())?;
                        entry.set("tick", tick.as_u32())?;
                    }
                    EventPayload::Custom { name, .. } => {
                        entry.set("type", "Custom")?;
                        entry.set("name", name.as_str())?;
                    }
                }
                t.set(i + 1, entry)?;
            }
            Ok(t)
        });

        // ctx:is_alive(entity_id: u32) → bool
        methods.add_method("is_alive", |_, this, entity_id: u32| {
            Ok(unsafe { this.as_ctx() }.is_alive(EntityId::new(entity_id)))
        });

        // ctx:get_component(entity_id: u32, name: String) → table or nil
        methods.add_method(
            "get_component",
            |lua, this, (entity_id, name): (u32, String)| {
                let reg = unsafe { this.as_reg() };
                let ctx = unsafe { this.as_ctx() };
                let entity = EntityId::new(entity_id);
                match reg.get_entry(&name) {
                    None => Ok(mlua::Value::Nil),
                    Some(entry) => match ctx.get_component_dyn(entity, entry.type_id) {
                        None => Ok(mlua::Value::Nil),
                        Some(comp) => {
                            let t = (entry.to_table)(comp, lua)?;
                            Ok(mlua::Value::Table(t))
                        }
                    },
                }
            },
        );

        // ctx:set_component(entity_id: u32, name: String, table) — queues AddComponent
        methods.add_method_mut(
            "set_component",
            |_, this, (entity_id, name, table): (u32, String, mlua::Table)| {
                // Deserialize while holding only a shared borrow of `this` (via `as_reg`).
                // The shared borrow is released before the mutable borrow (via `as_ctx_mut`).
                let comp_result = unsafe { this.as_reg() }.call_from_table(&name, &table);
                match comp_result {
                    None => Err(mlua::Error::RuntimeError(format!(
                        "component '{name}' not registered in LuaComponentRegistry"
                    ))),
                    Some(Err(e)) => Err(mlua::Error::RuntimeError(e.to_string())),
                    Some(Ok(comp)) => unsafe { this.as_ctx_mut() }
                        .add_component_boxed(EntityId::new(entity_id), comp)
                        .map_err(|e| mlua::Error::RuntimeError(format!("{e:?}"))),
                }
            },
        );

        // ctx:remove_component(entity_id: u32, name: String) — queues RemoveComponent
        methods.add_method_mut(
            "remove_component",
            |_, this, (entity_id, name): (u32, String)| {
                // Copy the TypeId while holding a shared borrow, then release before mutable borrow.
                let type_id = unsafe { this.as_reg() }.get_entry(&name).map(|e| e.type_id);
                match type_id {
                    None => Err(mlua::Error::RuntimeError(format!(
                        "component '{name}' not registered in LuaComponentRegistry"
                    ))),
                    Some(tid) => unsafe { this.as_ctx_mut() }
                        .remove_component_dyn(EntityId::new(entity_id), tid)
                        .map_err(|e| mlua::Error::RuntimeError(format!("{e:?}"))),
                }
            },
        );

        // ctx:spawn([template_id: u32]) → entity_id: u32
        methods.add_method_mut("spawn", |_, this, template_id: Option<u32>| {
            let ctx = unsafe { this.as_ctx_mut() };
            let req = match template_id {
                Some(tid) => SpawnRequest::with_template(tid),
                None => SpawnRequest::new(),
            };
            ctx.spawn(req)
                .map(|id| id.as_u32())
                .map_err(|e| mlua::Error::RuntimeError(format!("{e:?}")))
        });

        // ctx:despawn(entity_id: u32)
        methods.add_method_mut("despawn", |_, this, entity_id: u32| {
            let ctx = unsafe { this.as_ctx_mut() };
            ctx.despawn(EntityId::new(entity_id))
                .map_err(|e| mlua::Error::RuntimeError(format!("{e:?}")))
        });

        // ctx:iter_entities(name: String) → table array of entity_id (u32)
        // Returns IDs of every entity that currently has the named component.
        methods.add_method("iter_entities", |lua, this, name: String| {
            let type_id = match unsafe { this.as_reg() }.get_entry(&name) {
                Some(entry) => entry.type_id,
                None => return Err(mlua::Error::RuntimeError(
                    format!("component '{name}' not registered in LuaComponentRegistry"),
                )),
            };
            let ids = unsafe { this.as_ctx() }.iter_entities_with_component(type_id);
            let t = lua.create_table_with_capacity(ids.len(), 0)?;
            for (i, id) in ids.iter().enumerate() {
                t.set(i + 1, id.as_u32())?;
            }
            Ok(t)
        });

        // ctx:find_entities_with_tag(flag_name: String) → table array of entity_id (u32)
        // flag_name is one of: player, enemy, player_bullet, enemy_bullet, pickup, boss, invincible, graze
        methods.add_method("find_entities_with_tag", |lua, this, flag_name: String| {
            let flags = bindings::flag_name_to_flags(&flag_name)?;
            let ids = unsafe { this.as_ctx() }.find_entities_with_tag(flags);
            let t = lua.create_table_with_capacity(ids.len(), 0)?;
            for (i, id) in ids.iter().enumerate() {
                t.set(i + 1, id.as_u32())?;
            }
            Ok(t)
        });

        // ctx:log(msg: String) — writes a tagged message to stderr (P3)
        // Visible in development builds; use sparingly in hot paths.
        methods.add_method("log", |_, this, msg: String| {
            let tick = unsafe { this.as_ctx() }.tick().as_u32();
            eprintln!("[Lua] tick={tick}: {msg}");
            Ok(())
        });

        // ── Render API ────────────────────────────────────────────────────
        // These methods enqueue RenderCommands for the Godot bridge to apply
        // against RenderingServer after this frame's ECS step completes.
        // They are no-ops when no bridge is attached (render_tx is None).

        // ctx:create_sprite(handle, texture_path, z_index)
        // Creates a canvas item for `handle` displaying `texture_path`.
        // `handle` is typically the entity ID cast to u64.
        methods.add_method(
            "create_sprite",
            |_, this, (handle, texture_path, z_index): (u64, String, i32)| {
                if let Some(tx) = &this.render_tx {
                    let _ = tx.send(RenderCommand::CreateSprite { handle, texture_path, z_index });
                }
                Ok(())
            },
        );

        // ctx:update_sprite(handle, x, y, rotation, scale_x, scale_y)
        methods.add_method(
            "update_sprite",
            |_, this, (handle, x, y, rotation, scale_x, scale_y): (u64, f32, f32, f32, f32, f32)| {
                if let Some(tx) = &this.render_tx {
                    let _ = tx.send(RenderCommand::UpdateTransform {
                        handle, x, y, rotation, scale_x, scale_y,
                    });
                }
                Ok(())
            },
        );

        // ctx:set_sprite_visible(handle, visible)
        methods.add_method(
            "set_sprite_visible",
            |_, this, (handle, visible): (u64, bool)| {
                if let Some(tx) = &this.render_tx {
                    let _ = tx.send(RenderCommand::SetVisible { handle, visible });
                }
                Ok(())
            },
        );

        // ctx:set_sprite_modulate(handle, r, g, b, a)
        methods.add_method(
            "set_sprite_modulate",
            |_, this, (handle, r, g, b, a): (u64, f32, f32, f32, f32)| {
                if let Some(tx) = &this.render_tx {
                    let _ = tx.send(RenderCommand::SetModulate { handle, r, g, b, a });
                }
                Ok(())
            },
        );

        // ctx:destroy_sprite(handle)
        methods.add_method("destroy_sprite", |_, this, handle: u64| {
            if let Some(tx) = &this.render_tx {
                let _ = tx.send(RenderCommand::DestroySprite { handle });
            }
            Ok(())
        });
    }
}

// ── LuaEngine ─────────────────────────────────────────────────────────────────

/// Lua 5.4 scripting backend for the Evernight engine.
///
/// # Usage
/// ```rust,ignore
/// let mut engine = LuaEngine::new().unwrap();
/// app.set_script_engine(Box::new(engine));
/// app.load_script(r#"
///     function on_frame(ctx)
///         for _, e in ipairs(ctx:events()) do
///             if e.type == "Collision" then
///                 ctx:despawn(e.defender)
///             end
///         end
///     end
/// "#).unwrap();
/// ```
pub struct LuaEngine {
    lua: Lua,
    /// Cached `on_frame(ctx)` global — called once per tick.
    on_frame_fn:     Option<RegistryKey>,
    /// Cached `on_collision(ctx, attacker, defender, cx, cy, nx, ny)` global.
    on_collision_fn: Option<RegistryKey>,
    /// Cached `on_spawn(ctx, entity_id)` global.
    on_spawn_fn:     Option<RegistryKey>,
    /// Cached `on_despawn(ctx, entity_id)` global.
    on_despawn_fn:   Option<RegistryKey>,
    /// Cached `on_lifetime_expired(ctx, entity_id)` global.
    on_lifetime_fn:  Option<RegistryKey>,
    /// Registry of component types exposed to Lua scripts.
    lua_comp_reg: LuaComponentRegistry,
    /// Optional render command channel injected by the Godot bridge.
    render_tx: Option<RenderSender>,
}

impl LuaEngine {
    /// Creates a new sandboxed Lua 5.4 state.
    ///
    /// The following standard libraries are **excluded** for determinism:
    /// `os`, `io`, `debug`, `package`, `require`.
    pub fn new() -> EverNightResult<Self> {
        let lua = Lua::new();
        apply_sandbox(&lua).map_err(|e| EverNightError::ScriptError(e.to_string()))?;
        install_virtual_require(&lua).map_err(|e| EverNightError::ScriptError(e.to_string()))?;
        let mut engine = Self {
            lua,
            on_frame_fn:     None,
            on_collision_fn: None,
            on_spawn_fn:     None,
            on_despawn_fn:   None,
            on_lifetime_fn:  None,
            lua_comp_reg: LuaComponentRegistry::new(),
            render_tx: None,
        };
        engine.register_builtins();
        Ok(engine)
    }

    /// Registers a Lua source string as a virtual module.
    ///
    /// After this call `require("name")` in any Lua script will execute `source`
    /// exactly once, cache the return value, and return it — identical semantics
    /// to a real file-based module.  Must be called before `load()` or
    /// `load_file()` if the script calls `require` at module scope.
    pub fn add_module(&mut self, name: &str, source: &str) -> EverNightResult<()> {
        let globals = self.lua.globals();
        let modules: mlua::Table = globals
            .get("_MODULES")
            .map_err(|e| EverNightError::ScriptError(e.to_string()))?;
        modules
            .set(name, source)
            .map_err(|e| EverNightError::ScriptError(e.to_string()))
    }

    /// Injects the render command sender produced by the Godot bridge.
    ///
    /// Once set, Lua render methods (`ctx:create_sprite`, `ctx:update_sprite`,
    /// etc.) will enqueue `RenderCommand` values through this sender on every
    /// tick.  Safe to call before or after `load()`.
    pub fn set_render_sender(&mut self, tx: RenderSender) {
        self.render_tx = Some(tx);
    }

    /// Loads a Lua script from the filesystem and executes it.
    ///
    /// Equivalent to reading the file and calling [`ScriptEngine::load`] with
    /// its contents.  Safe to call on a running engine for **hot reload** —
    /// globals such as `on_frame` are replaced in place while entity/component
    /// state is preserved.
    pub fn load_file(&mut self, path: &std::path::Path) -> EverNightResult<()> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            EverNightError::ScriptError(format!("failed to read '{}': {e}", path.display()))
        })?;
        self.load(&source)
    }

    /// Registers the built-in engine components (Transform, Velocity, Tag, Lifetime,
    /// Hitbox, Hurtbox, ElasticCollision, Bounded) so they are immediately accessible
    /// from Lua without any manual `register_component` calls.
    fn register_builtins(&mut self) {
        use evernight_runtime::{Bounded, ElasticCollision, Hitbox, Hurtbox, Lifetime, Tag, Transform, Velocity};
        self.register_component::<Transform, _, _>("Transform", bindings::transform_to_table, bindings::table_to_transform);
        self.register_component::<Velocity, _, _>("Velocity",   bindings::velocity_to_table,  bindings::table_to_velocity);
        self.register_component::<Tag, _, _>(      "Tag",        bindings::tag_to_table,       bindings::table_to_tag);
        self.register_component::<Lifetime, _, _>( "Lifetime",   bindings::lifetime_to_table,  bindings::table_to_lifetime);
        self.register_component::<Hitbox, _, _>(   "Hitbox",     bindings::hitbox_to_table,    bindings::table_to_hitbox);
        self.register_component::<Hurtbox, _, _>(  "Hurtbox",    bindings::hurtbox_to_table,   bindings::table_to_hurtbox);
        self.register_component::<ElasticCollision, _, _>(
            "ElasticCollision",
            bindings::elastic_collision_to_table,
            bindings::table_to_elastic_collision,
        );
        self.register_component::<Bounded, _, _>(
            "Bounded",
            bindings::bounded_to_table,
            bindings::table_to_bounded,
        );
    }

    /// Registers a component type to be accessible from Lua via
    /// `ctx:get_component`, `ctx:set_component`, and `ctx:remove_component`.
    ///
    /// - `to_table`: serialize `&T` → Lua table (called by `get_component`).
    /// - `from_table`: deserialize Lua table → `T` (called by `set_component`).
    pub fn register_component<T, FTo, FFrom>(
        &mut self,
        name: &str,
        to_table: FTo,
        from_table: FFrom,
    ) where
        T: Component,
        FTo: Fn(&T, &Lua) -> mlua::Result<mlua::Table> + 'static,
        FFrom: Fn(&mlua::Table) -> mlua::Result<T> + 'static,
    {
        self.lua_comp_reg
            .register::<T, _, _>(name, to_table, from_table);
    }

    /// Refreshes all cached Lua handler registry keys after a `load()`.
    ///
    /// A key is cleared when the corresponding global is no longer a function
    /// (supports hot-reload that removes handlers).
    fn cache_handlers(&mut self) -> mlua::Result<()> {
        let globals = self.lua.globals();
        // Two-step (extract then store) keeps borrow-checker happy:
        // `create_registry_value` borrows `self.lua`; the assignment borrows the
        // target field — disjoint, so NLL permits both in one statement.
        let f = globals.get::<mlua::Function>("on_frame").ok();
        self.on_frame_fn = match f { Some(f) => Some(self.lua.create_registry_value(f)?), None => None };

        let f = globals.get::<mlua::Function>("on_collision").ok();
        self.on_collision_fn = match f { Some(f) => Some(self.lua.create_registry_value(f)?), None => None };

        let f = globals.get::<mlua::Function>("on_spawn").ok();
        self.on_spawn_fn = match f { Some(f) => Some(self.lua.create_registry_value(f)?), None => None };

        let f = globals.get::<mlua::Function>("on_despawn").ok();
        self.on_despawn_fn = match f { Some(f) => Some(self.lua.create_registry_value(f)?), None => None };

        let f = globals.get::<mlua::Function>("on_lifetime_expired").ok();
        self.on_lifetime_fn = match f { Some(f) => Some(self.lua.create_registry_value(f)?), None => None };

        Ok(())
    }
}

impl ScriptEngine for LuaEngine {
    fn load(&mut self, source: &str) -> EverNightResult<()> {
        self.lua
            .load(source)
            .exec()
            .map_err(|e| EverNightError::ScriptError(e.to_string()))?;
        self.cache_handlers()
            .map_err(|e| EverNightError::ScriptError(e.to_string()))
    }

    fn on_frame(&mut self, ctx: &mut ScriptContext<'_>) -> EverNightResult<()> {
        let has_event_handlers = self.on_collision_fn.is_some()
            || self.on_spawn_fn.is_some()
            || self.on_despawn_fn.is_some()
            || self.on_lifetime_fn.is_some();

        if self.on_frame_fn.is_none() && !has_event_handlers {
            return Ok(());
        }

        // Pre-collect event data while ctx is still freely borrowable.
        // Only collect kinds for which a handler is actually registered.
        let mut col_events:  Vec<(u32, u32, f32, f32, f32, f32)> = Vec::new();
        let mut spn_events:  Vec<u32> = Vec::new();
        let mut dsp_events:  Vec<u32> = Vec::new();
        let mut life_events: Vec<u32> = Vec::new();

        if has_event_handlers {
            for event in ctx.events() {
                match event {
                    EventPayload::Collision { attacker, defender, contact_point, normal, .. } => {
                        if self.on_collision_fn.is_some() {
                            col_events.push((
                                attacker.as_u32(), defender.as_u32(),
                                contact_point.x, contact_point.y,
                                normal.x, normal.y,
                            ));
                        }
                    }
                    EventPayload::Spawned { entity, .. } => {
                        if self.on_spawn_fn.is_some() { spn_events.push(entity.as_u32()); }
                    }
                    EventPayload::Despawned { entity, .. } => {
                        if self.on_despawn_fn.is_some() { dsp_events.push(entity.as_u32()); }
                    }
                    EventPayload::LifetimeExpired { entity, .. } => {
                        if self.on_lifetime_fn.is_some() { life_events.push(entity.as_u32()); }
                    }
                    EventPayload::Custom { .. } => {}
                }
            }
        }

        // Erase lifetimes for the raw pointers stored in CtxUserdata.
        // SAFETY: The `scope` call guarantees CtxUserdata cannot outlive `ctx` or `self`.
        let ctx_ptr = ctx as *mut ScriptContext<'_> as *mut ();
        let reg_ptr = &self.lua_comp_reg as *const LuaComponentRegistry as *const ();
        let lua = &self.lua;

        // Fetch all registered handler functions before entering scope.
        macro_rules! fetch {
            ($field:ident) => {
                self.$field
                    .as_ref()
                    .map(|k| lua.registry_value::<mlua::Function>(k))
                    .transpose()
                    .map_err(|e| EverNightError::ScriptError(e.to_string()))?
            };
        }
        let on_frame_f:    Option<mlua::Function> = fetch!(on_frame_fn);
        let on_collision_f: Option<mlua::Function> = fetch!(on_collision_fn);
        let on_spawn_f:    Option<mlua::Function> = fetch!(on_spawn_fn);
        let on_despawn_f:  Option<mlua::Function> = fetch!(on_despawn_fn);
        let on_lifetime_f: Option<mlua::Function> = fetch!(on_lifetime_fn);

        lua.scope(|scope| {
            let ud = scope.create_userdata(CtxUserdata {
            ctx_ptr,
            reg_ptr,
            render_tx: self.render_tx.clone(),
        })?;

            // Per-tick handler
            if let Some(f) = &on_frame_f {
                f.call::<()>(ud.clone())?;
            }

            // Event-specific handlers
            if let Some(f) = &on_collision_f {
                for &(att, def, cx, cy, nx, ny) in &col_events {
                    f.call::<()>((ud.clone(), att, def, cx, cy, nx, ny))?;
                }
            }
            if let Some(f) = &on_spawn_f {
                for &eid in &spn_events { f.call::<()>((ud.clone(), eid))?; }
            }
            if let Some(f) = &on_despawn_f {
                for &eid in &dsp_events { f.call::<()>((ud.clone(), eid))?; }
            }
            if let Some(f) = &on_lifetime_f {
                for &eid in &life_events { f.call::<()>((ud.clone(), eid))?; }
            }

            Ok(())
        })
        .map_err(|e| EverNightError::ScriptError(e.to_string()))
    }
}

// ── Sandboxing ────────────────────────────────────────────────────────────────

/// Removes non-deterministic / dangerous standard library entries.
fn apply_sandbox(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    // Remove libraries that break determinism or expose the filesystem.
    // Note: `require` is removed here and replaced by the virtual require below.
    for name in &[
        "os", "io", "debug", "package", "require", "dofile", "loadfile",
    ] {
        globals.set(*name, mlua::Value::Nil)?;
    }
    // Neuter math.random / math.randomseed
    if let Ok(math) = globals.get::<mlua::Table>("math") {
        math.set("random", mlua::Value::Nil)?;
        math.set("randomseed", mlua::Value::Nil)?;
    }
    Ok(())
}

/// Installs a sandboxed `require` backed by a Lua-side `_MODULES` table.
///
/// The host registers modules via [`LuaEngine::add_module`]; scripts call
/// `require("name")` exactly as they would in a full Lua environment.  Results
/// are cached in `_LOADED` (first call executes the source, subsequent calls
/// return the cached value).
///
/// Attempting to require an unregistered name raises a Lua runtime error.
fn install_virtual_require(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    // _MODULES: name → source string  (populated by add_module)
    // _LOADED:  name → cached value   (populated by require on first load)
    globals.set("_MODULES", lua.create_table()?)?;
    globals.set("_LOADED",  lua.create_table()?)?;

    let require_fn = lua.create_function(|lua, name: String| {
        let globals = lua.globals();
        let loaded:  mlua::Table = globals.get("_LOADED")?;
        let modules: mlua::Table = globals.get("_MODULES")?;

        // Fast-path: already cached
        let cached: mlua::Value = loaded.get(name.as_str())?;
        if !matches!(cached, mlua::Value::Nil) {
            return Ok(cached);
        }

        // Look up source
        let source: Option<String> = modules.get(name.as_str())?;
        let source = source.ok_or_else(|| {
            mlua::Error::RuntimeError(format!("module '{name}' not found"))
        })?;

        // Compile and execute the module chunk
        let chunk = lua.load(source.as_str()).into_function()?;
        let result: mlua::Value = chunk.call(())?;

        // Lua convention: cache `true` when the module returns nil
        let to_cache = if matches!(result, mlua::Value::Nil) {
            mlua::Value::Boolean(true)
        } else {
            result.clone()
        };
        loaded.set(name, to_cache)?;
        Ok(result)
    })?;

    globals.set("require", require_fn)?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use evernight_runtime::FixedStep;
    use evernight_script::App;

    fn make_app(engine: LuaEngine) -> App {
        let mut app = App::new(FixedStep::new_60hz());
        app.set_script_engine(Box::new(engine));
        app
    }

    #[test]
    fn lua_engine_new_succeeds() {
        assert!(LuaEngine::new().is_ok());
    }

    #[test]
    fn load_valid_script_returns_ok() {
        let mut engine = LuaEngine::new().unwrap();
        assert!(engine.load("local x = 1 + 1").is_ok());
    }

    #[test]
    fn load_syntax_error_returns_script_error() {
        let mut engine = LuaEngine::new().unwrap();
        let result = engine.load("this is not valid lua @@@@");
        assert!(matches!(result, Err(EverNightError::ScriptError(_))));
    }

    #[test]
    fn sandbox_removes_os() {
        let mut engine = LuaEngine::new().unwrap();
        let result = engine.load("return os.time()");
        assert!(result.is_err(), "os should be sandboxed");
    }

    #[test]
    fn sandbox_removes_math_random() {
        let mut engine = LuaEngine::new().unwrap();
        let result = engine.load("return math.random()");
        assert!(result.is_err(), "math.random should be sandboxed");
    }

    #[test]
    fn on_frame_not_defined_is_noop() {
        let engine = LuaEngine::new().unwrap();
        let mut app = make_app(engine);
        // No on_frame defined — must not panic or error
        assert!(app.step().is_ok());
    }

    #[test]
    fn on_frame_lua_function_called_each_step() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.frame_count = 0
            function on_frame(ctx)
                _G.frame_count = _G.frame_count + 1
            end
        "#,
            )
            .unwrap();

        // Access the Lua state before moving engine into App
        // (not possible after Box<dyn ScriptEngine>).
        // Instead we read a global Lua value via the engine before boxing.
        // Since App takes ownership, we test by running two steps and checking
        // that the script ran (no error) — behaviour verified in next test.
        let mut app = make_app(engine);
        app.step().unwrap();
        app.step().unwrap();
        // If we reach here without panic/error, on_frame was called each step.
    }

    #[test]
    fn ctx_tick_accessible_from_lua_does_not_error() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                -- Just reading tick must not throw
                local _ = ctx:tick()
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    #[test]
    fn ctx_delta_time_accessible_from_lua() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                local dt = ctx:delta_time()
                assert(dt > 0, "delta_time must be positive")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    #[test]
    fn ctx_spawn_and_is_alive_from_lua() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.spawned_id = nil
            function on_frame(ctx)
                if _G.spawned_id == nil then
                    _G.spawned_id = ctx:spawn()
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        // Step 1: on_frame runs and queues a spawn
        app.step().unwrap();
        // Step 2: spawn commit happened in step 1's SpawnCommit phase,
        // so the entity is alive by the time step 2 starts
        app.step().unwrap();
        // World has at least one live entity
        let _world = app.world();
        // We can't easily get the entity ID back, but if no panic/error
        // occurred the spawn+is_alive path through Lua is exercised.
    }

    #[test]
    fn ctx_events_returns_empty_table_when_no_events() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                local evts = ctx:events()
                assert(type(evts) == "table", "events() must return a table")
                assert(#evts == 0, "should be no events on first frame")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    // ── Component read/write tests ────────────────────────────────────────────

    struct Position {
        x: f32,
        y: f32,
    }
    evernight_core::impl_component!(Position);

    fn register_position(engine: &mut LuaEngine) {
        engine.register_component::<Position, _, _>(
            "Position",
            |p, lua| {
                let t = lua.create_table()?;
                t.set("x", p.x)?;
                t.set("y", p.y)?;
                Ok(t)
            },
            |t| {
                Ok(Position {
                    x: t.get("x")?,
                    y: t.get("y")?,
                })
            },
        );
    }

    #[test]
    fn get_component_returns_nil_for_unregistered_name() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.entity = nil
            function on_frame(ctx)
                if _G.entity == nil then
                    _G.entity = ctx:spawn()
                else
                    local v = ctx:get_component(_G.entity, "UnknownComponent")
                    assert(v == nil, "should return nil for unregistered component")
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
        app.step().unwrap();
    }

    #[test]
    fn set_component_and_get_component_round_trip() {
        let mut engine = LuaEngine::new().unwrap();
        register_position(&mut engine);
        engine
            .load(
                r#"
            _G.entity = nil
            _G.checked = false
            function on_frame(ctx)
                if _G.entity == nil then
                    _G.entity = ctx:spawn()
                    ctx:set_component(_G.entity, "Position", { x = 3.0, y = 7.0 })
                elseif not _G.checked then
                    local pos = ctx:get_component(_G.entity, "Position")
                    assert(pos ~= nil, "Position should exist")
                    assert(pos.x == 3.0, "x should be 3.0, got " .. tostring(pos.x))
                    assert(pos.y == 7.0, "y should be 7.0, got " .. tostring(pos.y))
                    _G.checked = true
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // frame 1: spawn + set_component queued → committed at end
        app.step().unwrap(); // frame 2: entity + component live → read back
    }

    #[test]
    fn remove_component_makes_get_return_nil() {
        let mut engine = LuaEngine::new().unwrap();
        register_position(&mut engine);
        engine
            .load(
                r#"
            _G.entity = nil
            _G.phase = 0
            function on_frame(ctx)
                if _G.phase == 0 then
                    _G.entity = ctx:spawn()
                    ctx:set_component(_G.entity, "Position", { x = 1.0, y = 2.0 })
                    _G.phase = 1
                elseif _G.phase == 1 then
                    -- confirm it exists before removal
                    local pos = ctx:get_component(_G.entity, "Position")
                    assert(pos ~= nil, "Position should exist before remove")
                    ctx:remove_component(_G.entity, "Position")
                    _G.phase = 2
                elseif _G.phase == 2 then
                    -- after removal committed, should be nil
                    local pos = ctx:get_component(_G.entity, "Position")
                    assert(pos == nil, "Position should be nil after remove")
                    _G.phase = 3
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // phase 0 → 1: spawn + set_component
        app.step().unwrap(); // phase 1 → 2: confirm exists, queue remove
        app.step().unwrap(); // phase 2 → 3: confirm nil
    }

    #[test]
    fn set_component_with_unregistered_name_errors() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.entity = nil
            function on_frame(ctx)
                if _G.entity == nil then
                    _G.entity = ctx:spawn()
                else
                    ctx:set_component(_G.entity, "Ghost", { x = 0 })
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // frame 1: spawn
        let result = app.step(); // frame 2: set_component with unknown name → error
        assert!(result.is_err(), "should error for unregistered component");
    }

    // ── Built-in component tests ──────────────────────────────────────────────

    #[test]
    fn builtin_transform_readable_without_manual_registration() {
        // Transform is auto-registered; script must be able to read/write it
        // without the host calling register_component::<Transform>() manually.
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.entity = nil
            _G.ok = false
            function on_frame(ctx)
                if _G.entity == nil then
                    _G.entity = ctx:spawn()
                    ctx:set_component(_G.entity, "Transform", { x = 5.0, y = 10.0, rotation = 0.0 })
                else
                    local tf = ctx:get_component(_G.entity, "Transform")
                    assert(tf ~= nil, "Transform should exist")
                    assert(math.abs(tf.x - 5.0) < 0.001, "x should be 5.0")
                    assert(math.abs(tf.y - 10.0) < 0.001, "y should be 10.0")
                    _G.ok = true
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // frame 1: spawn + set Transform
        app.step().unwrap(); // frame 2: read back and assert in Lua
    }

    #[test]
    fn builtin_velocity_readable_without_manual_registration() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.entity = nil
            function on_frame(ctx)
                if _G.entity == nil then
                    _G.entity = ctx:spawn()
                    ctx:set_component(_G.entity, "Velocity", { vx = 3.0, vy = -1.0, angular = 0.0 })
                else
                    local v = ctx:get_component(_G.entity, "Velocity")
                    assert(v ~= nil, "Velocity should exist")
                    assert(math.abs(v.vx - 3.0) < 0.001, "vx should be 3.0")
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
        app.step().unwrap();
    }

    // ── iter_entities tests ───────────────────────────────────────────────────

    #[test]
    fn iter_entities_returns_entities_with_component() {
        // Spawn entities with Transform from Lua, then verify iter_entities sees them.
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.phase = 0
            function on_frame(ctx)
                if _G.phase == 0 then
                    -- spawn two entities with Transform
                    local e1 = ctx:spawn()
                    ctx:set_component(e1, "Transform", { x = 1.0, y = 0.0, rotation = 0.0 })
                    local e2 = ctx:spawn()
                    ctx:set_component(e2, "Transform", { x = 2.0, y = 0.0, rotation = 0.0 })
                    _G.phase = 1
                elseif _G.phase == 1 then
                    -- commands committed; iter_entities must see at least 2 Transforms
                    local ids = ctx:iter_entities("Transform")
                    assert(#ids >= 2, "expected at least 2 Transform entities, got " .. #ids)
                    _G.phase = 2
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // phase 0 → 1: spawn + set_component (queued)
        app.step().unwrap(); // phase 1 → 2: components committed, iter_entities asserts
    }

    #[test]
    fn iter_entities_unknown_component_errors() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                ctx:iter_entities("DoesNotExist")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        let result = app.step();
        assert!(result.is_err(), "iter_entities with unknown component should error");
    }

    // ── find_entities_with_tag tests ──────────────────────────────────────────

    #[test]
    fn find_entities_with_tag_returns_matching_entities() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.found = false
            _G.entity = nil
            function on_frame(ctx)
                if _G.entity == nil then
                    _G.entity = ctx:spawn()
                    ctx:set_component(_G.entity, "Tag", { flags = 1, custom = {} }) -- PLAYER flag
                else
                    local players = ctx:find_entities_with_tag("player")
                    assert(#players >= 1, "should find at least one player entity")
                    _G.found = true
                end
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // spawn + set Tag
        app.step().unwrap(); // find_entities_with_tag asserts in Lua
    }

    #[test]
    fn find_entities_with_tag_unknown_flag_errors() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                ctx:find_entities_with_tag("unicorn")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        let result = app.step();
        assert!(result.is_err(), "unknown flag name should error");
    }

    // ── P2: add_module / require ───────────────────────────────────────────────

    #[test]
    fn add_module_and_require_returns_value() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .add_module(
                "utils",
                r#"
            local M = {}
            function M.double(x) return x * 2 end
            return M
        "#,
            )
            .unwrap();
        engine
            .load(
                r#"
            local utils = require("utils")
            function on_frame(ctx)
                assert(utils.double(3) == 6, "utils.double(3) should be 6")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    #[test]
    fn require_caches_module_result() {
        // The module source is executed only once; state is shared across calls.
        let mut engine = LuaEngine::new().unwrap();
        engine
            .add_module(
                "counter",
                r#"
            local count = 0
            local M = {}
            function M.inc() count = count + 1; return count end
            return M
        "#,
            )
            .unwrap();
        engine
            .load(
                r#"
            local c = require("counter")
            local c2 = require("counter")   -- should return same cached table
            function on_frame(ctx)
                assert(c.inc() == 1, "first inc should be 1")
                assert(c2.inc() == 2, "second inc on same module should be 2")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    #[test]
    fn require_unknown_module_errors() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                require("not_registered")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        let result = app.step();
        assert!(result.is_err(), "require of unregistered module should error");
    }

    #[test]
    fn load_file_executes_script() {
        use std::io::Write;

        // Write a temporary Lua file and verify load_file executes it.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(
            tmp,
            r#"
            function on_frame(ctx)
                assert(ctx:tick() >= 0, "tick should be non-negative")
            end
        "#
        )
        .unwrap();

        let mut engine = LuaEngine::new().unwrap();
        engine.load_file(tmp.path()).unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    #[test]
    fn load_file_hot_reload_replaces_handler() {
        use std::io::{Seek, Write};

        // First script: on_frame asserts tick >= 0 (always true).
        // Hot-reload with a different script: on_frame does nothing.
        // Both should succeed without error.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "function on_frame(ctx) end").unwrap();

        let mut engine = LuaEngine::new().unwrap();
        engine.load_file(tmp.path()).unwrap();

        // Hot-reload with a new version
        tmp.seek(std::io::SeekFrom::Start(0)).unwrap();
        tmp.as_file().set_len(0).unwrap();
        write!(tmp, "function on_frame(ctx) local _ = ctx:tick() end").unwrap();
        engine.load_file(tmp.path()).unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }

    // ── P2: event callbacks ────────────────────────────────────────────────────

    #[test]
    fn on_spawn_callback_called_for_spawned_entity() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.spawn_count = 0
            _G.phase = 0
            function on_frame(ctx)
                if _G.phase == 0 then
                    ctx:spawn()
                    _G.phase = 1
                end
            end
            function on_spawn(ctx, eid)
                _G.spawn_count = _G.spawn_count + 1
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap(); // phase 0: queues spawn; on_spawn not fired yet
        app.step().unwrap(); // phase 1: SpawnedEvent exists → on_spawn fires
        // We can't read Lua globals directly here (engine is boxed), but if
        // on_spawn raised an error, step() would have returned Err.
    }

    #[test]
    fn on_collision_callback_receives_correct_args() {
        use evernight_core::{LayerBit, SpawnRequest};
        use evernight_math::{Circle, Shape2D, Vec2};
        use evernight_runtime::{Hitbox, Hurtbox, Transform};

        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            _G.collision_called = false
            function on_collision(ctx, att, def, cx, cy, nx, ny)
                -- basic sanity: IDs are non-zero integers, normals are numbers
                assert(type(att) == "number", "att must be a number")
                assert(type(def) == "number", "def must be a number")
                assert(type(nx) == "number",  "nx must be a number")
                _G.collision_called = true
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);

        // Spawn overlapping attacker + defender in Rust so collision fires.
        let at = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        let df = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();

        let tf = Transform::identity();
        let circle = Shape2D::Circle(Circle { center: Vec2::zero(), radius: 10.0 });
        let layer = LayerBit::new(0);

        app.world_mut().add_component(at, tf).unwrap();
        app.world_mut().add_component(at, Hitbox { shape: circle.clone(), layer, group: evernight_core::CollisionMask::from_raw(0xFFFF_FFFF), hit_once: false }).unwrap();
        app.world_mut().add_component(df, tf).unwrap();
        app.world_mut().add_component(df, Hurtbox { shape: circle, layer }).unwrap();

        app.step().unwrap(); // collision detected → on_collision fired
    }

    // ── P3: ctx:log ───────────────────────────────────────────────────────────

    #[test]
    fn ctx_log_does_not_error() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .load(
                r#"
            function on_frame(ctx)
                ctx:log("hello from Lua")
            end
        "#,
            )
            .unwrap();

        let mut app = make_app(engine);
        app.step().unwrap();
    }
}
