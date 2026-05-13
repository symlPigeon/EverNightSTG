use evernight_core::EverNightResult;

use crate::ScriptContext;

/// Abstraction for a scripting-language backend.
///
/// Implement this trait to plug any scripting runtime (Lua, Wren, Rhai, …)
/// into [`App`].  The implementation lives in a separate crate (e.g.
/// `evernight_lua`) so that `evernight_script` stays dependency-free.
///
/// # Lifecycle
/// 1. Construct the concrete engine and call [`App::set_script_engine`].
/// 2. Call [`App::load_script`] (or `engine.load()` directly) for each source file.
/// 3. [`App::step`] calls [`ScriptEngine::on_frame`] once per tick,
///    **after** collision events are emitted and PostCollision hooks have run,
///    **before** the lifetime (despawn) phase.
///
/// # Example (mock backend)
/// ```rust,ignore
/// struct NoopEngine;
/// impl ScriptEngine for NoopEngine {
///     fn load(&mut self, _source: &str) -> EverNightResult<()> { Ok(()) }
///     fn on_frame(&mut self, _ctx: &mut ScriptContext<'_>) -> EverNightResult<()> { Ok(()) }
/// }
/// app.set_script_engine(Box::new(NoopEngine));
/// ```
pub trait ScriptEngine {
    /// Compile and register a script from source text.
    ///
    /// May be called multiple times to load additional scripts.
    /// The exact semantics (namespace isolation, re-load behaviour) are
    /// implementation-defined.
    fn load(&mut self, source: &str) -> EverNightResult<()>;

    /// Called once per game tick with a live [`ScriptContext`].
    ///
    /// The engine should dispatch all per-frame scripted logic here
    /// (update handlers, event reactions, spawning, etc.).
    ///
    /// Returning `Err` propagates out of [`App::step`] and halts the frame.
    fn on_frame(&mut self, ctx: &mut ScriptContext<'_>) -> EverNightResult<()>;
}
