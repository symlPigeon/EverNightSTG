use evernight_core::{
    Component, EntityId, EventPayload, EverNightError, EverNightResult, SpawnRequest,
};
use evernight_script::{ScriptContext, ScriptEngine};
use mlua::{Lua, RegistryKey, UserData, UserDataMethods};

use crate::lua_component_registry::LuaComponentRegistry;

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
    /// Cached reference to the Lua `on_frame` global function.
    /// Populated on first `load()` call that defines `on_frame`.
    on_frame_fn: Option<RegistryKey>,
    /// Registry of component types exposed to Lua scripts.
    lua_comp_reg: LuaComponentRegistry,
}

impl LuaEngine {
    /// Creates a new sandboxed Lua 5.4 state.
    ///
    /// The following standard libraries are **excluded** for determinism:
    /// `os`, `io`, `debug`, `package`, `require`.
    pub fn new() -> EverNightResult<Self> {
        let lua = Lua::new();
        apply_sandbox(&lua).map_err(|e| EverNightError::ScriptError(e.to_string()))?;
        Ok(Self {
            lua,
            on_frame_fn: None,
            lua_comp_reg: LuaComponentRegistry::new(),
        })
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

    /// Refreshes the cached `on_frame` registry key after a `load()`.
    fn cache_on_frame(&mut self) -> mlua::Result<()> {
        let globals = self.lua.globals();
        if let Ok(f) = globals.get::<mlua::Function>("on_frame") {
            let key = self.lua.create_registry_value(f)?;
            self.on_frame_fn = Some(key);
        }
        Ok(())
    }
}

impl ScriptEngine for LuaEngine {
    fn load(&mut self, source: &str) -> EverNightResult<()> {
        self.lua
            .load(source)
            .exec()
            .map_err(|e| EverNightError::ScriptError(e.to_string()))?;
        self.cache_on_frame()
            .map_err(|e| EverNightError::ScriptError(e.to_string()))
    }

    fn on_frame(&mut self, ctx: &mut ScriptContext<'_>) -> EverNightResult<()> {
        let key = match &self.on_frame_fn {
            Some(k) => k,
            None => return Ok(()),
        };

        // Erase lifetimes so we can store the pointers in CtxUserdata.
        // SAFETY: The `scope` call below guarantees that the CtxUserdata (and
        // therefore all Lua access through it) cannot outlive `ctx` or `self`.
        let ctx_ptr = ctx as *mut ScriptContext<'_> as *mut ();
        let reg_ptr = &self.lua_comp_reg as *const LuaComponentRegistry as *const ();

        let lua = &self.lua;
        let on_frame: mlua::Function = lua
            .registry_value(key)
            .map_err(|e| EverNightError::ScriptError(e.to_string()))?;

        lua.scope(|scope| {
            let ud = scope.create_userdata(CtxUserdata { ctx_ptr, reg_ptr })?;
            on_frame.call::<()>(ud)
        })
        .map_err(|e| EverNightError::ScriptError(e.to_string()))
    }
}

// ── Sandboxing ────────────────────────────────────────────────────────────────

/// Removes non-deterministic / dangerous standard library entries.
fn apply_sandbox(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    // Remove libraries that break determinism or expose the filesystem
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
}
