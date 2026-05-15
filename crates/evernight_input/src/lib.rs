use std::collections::HashSet;

/// A snapshot of input state captured once per game tick before `App::step()`.
///
/// Platform-agnostic: action names are arbitrary strings matching the host
/// platform's input-action map (e.g. Godot project actions like `"move_left"`).
///
/// # Usage (Godot bridge)
/// ```rust,ignore
/// let mut snap = InputSnapshot::default();
/// snap.set_pressed("move_left",  input.is_action_pressed("move_left"));
/// snap.set_pressed("move_right", input.is_action_pressed("move_right"));
/// snap.set_just_pressed("fire",  input.is_action_just_pressed("fire"));
/// snap.set_just_released("fire", input.is_action_just_released("fire"));
/// app.set_input(snap);
/// app.step().unwrap();
/// ```
///
/// # Usage (Lua)
/// ```lua
/// function on_frame(ctx)
///     if ctx:is_key_pressed("move_left") then
///         -- move player
///     end
///     if ctx:is_key_just_pressed("fire") then
///         -- spawn bullet
///     end
/// end
/// ```
#[derive(Default, Clone, Debug)]
pub struct InputSnapshot {
    /// Actions whose button/key is held down this frame.
    pressed: HashSet<String>,
    /// Actions that transitioned from released → pressed this frame.
    just_pressed: HashSet<String>,
    /// Actions that transitioned from pressed → released this frame.
    just_released: HashSet<String>,
}

impl InputSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Setters (called by host platform before step) ─────────────────────

    /// Marks `action` as held / not held this frame.
    pub fn set_pressed(&mut self, action: impl Into<String>, is_pressed: bool) {
        let action = action.into();
        if is_pressed {
            self.pressed.insert(action);
        } else {
            self.pressed.remove(&action);
        }
    }

    /// Marks `action` as having been newly pressed this frame.
    pub fn set_just_pressed(&mut self, action: impl Into<String>, value: bool) {
        let action = action.into();
        if value {
            self.just_pressed.insert(action);
        } else {
            self.just_pressed.remove(&action);
        }
    }

    /// Marks `action` as having been released this frame.
    pub fn set_just_released(&mut self, action: impl Into<String>, value: bool) {
        let action = action.into();
        if value {
            self.just_released.insert(action);
        } else {
            self.just_released.remove(&action);
        }
    }

    // ── Queries (called by scripts) ───────────────────────────────────────

    /// Returns `true` if the action is currently held down.
    #[inline]
    pub fn is_pressed(&self, action: &str) -> bool {
        self.pressed.contains(action)
    }

    /// Returns `true` if the action was pressed for the first time this frame.
    #[inline]
    pub fn is_just_pressed(&self, action: &str) -> bool {
        self.just_pressed.contains(action)
    }

    /// Returns `true` if the action was released this frame.
    #[inline]
    pub fn is_just_released(&self, action: &str) -> bool {
        self.just_released.contains(action)
    }

    /// Clears `just_pressed` and `just_released` sets.
    /// Called by `App::step()` at the end of each frame so transient state
    /// does not bleed into the next tick.
    pub fn clear_transients(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}
