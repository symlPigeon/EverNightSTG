use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};

use godot::classes::{INode, Input, Node, ProjectSettings, RenderingServer, Texture2D};
use godot::obj::Singleton;
use godot::prelude::*;

use evernight_input::InputSnapshot;
use evernight_lua::{
    LuaEngine,
    render_cmd::{RenderCommand, RenderSender},
};
use evernight_runtime::FixedStep;
use evernight_script::{App, ScriptEngine};

// ── EvernightBridge ───────────────────────────────────────────────────────────

/// Godot node that owns the ECS simulation and drives `RenderingServer`.
///
/// # Usage (GDScript)
/// ```gdscript
/// # In a script attached to an EvernightBridge node:
/// func _ready():
///     lua_source = """
///         function on_frame(ctx) end
///     """
/// ```
///
/// # Threading model
/// The ECS (`App` + `LuaEngine`) runs synchronously inside Godot's `_process`
/// callback on the **main thread**.  Lua render methods enqueue `RenderCommand`
/// values via an mpsc channel; the bridge drains that channel immediately after
/// each `app.step()` call and applies the commands to `RenderingServer` — all
/// within the same frame, on the same thread.
///
/// This avoids any cross-thread `RenderingServer` access and keeps the Lua
/// runtime (`!Send`) on the main thread.  A future migration to a dedicated ECS
/// thread would require enabling mlua's `send` feature.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct EvernightBridge {
    base: Base<Node>,

    /// The ECS simulation. `None` until `_ready`.
    app: Option<App>,

    /// Receiving end of the render command channel.
    receiver: Option<Receiver<RenderCommand>>,

    /// Single shared canvas attached to the root viewport. `None` until `_ready`.
    canvas: Option<Rid>,

    /// Maps Lua-assigned sprite handles → canvas item RIDs.
    rid_map: HashMap<u64, Rid>,

    /// Texture resource cache keyed by `res://` path.
    /// Keeps `Gd<Texture2D>` alive so Godot doesn't unload the resource.
    texture_cache: HashMap<String, Gd<Texture2D>>,

    /// Lua source to load on `_ready`.  Set this in the Godot inspector or
    /// from GDScript before the node enters the scene tree.
    #[export]
    lua_source: GString,

    /// List of Godot input-action names to poll each frame.
    /// Add your project's action names here (e.g. `"move_left"`, `"fire"`) so
    /// they are available to Lua via `ctx:is_key_pressed(action)`.
    #[export]
    input_actions: Array<GString>,
}

#[godot_api]
impl INode for EvernightBridge {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            app: None,
            receiver: None,
            canvas: None,
            rid_map: HashMap::new(),
            texture_cache: HashMap::new(),
            lua_source: GString::new(),
            input_actions: Array::new(),
        }
    }

    fn ready(&mut self) {
        // ── Render channel ────────────────────────────────────────────────
        let (tx, rx): (RenderSender, Receiver<RenderCommand>) = mpsc::channel();
        self.receiver = Some(rx);

        // ── Lua engine ────────────────────────────────────────────────────
        let mut engine = match LuaEngine::new() {
            Ok(e) => e,
            Err(e) => {
                godot_error!("[Evernight] LuaEngine init failed: {e:?}");
                return;
            }
        };
        engine.set_render_sender(tx);

        let src = self.lua_source.to_string();
        if !src.is_empty() {
            if let Err(e) = engine.load(&src) {
                godot_error!("[Evernight] Script error: {e:?}");
            }
        }

        // ── ECS App ───────────────────────────────────────────────────────
        let mut app = App::new(FixedStep::new_60hz());
        app.set_script_engine(Box::new(engine));
        self.app = Some(app);

        // ── RenderingServer canvas ────────────────────────────────────────
        let mut rs = RenderingServer::singleton();
        let canvas = rs.canvas_create();
        if let Some(vp) = self.base().get_viewport() {
            let vp_rid = vp.get_viewport_rid();
            rs.viewport_attach_canvas(vp_rid, canvas);

            // Shift canvas origin to viewport centre so that Lua world (0,0) = screen centre.
            let size = vp.get_visible_rect().size;
            let centered = Transform2D::from_cols(
                Vector2::new(1.0, 0.0),
                Vector2::new(0.0, 1.0),
                Vector2::new(size.x / 2.0, size.y / 2.0),
            );
            rs.viewport_set_canvas_transform(vp_rid, canvas, centered);
        } else {
            godot_warn!("[Evernight] No viewport found; canvas not attached.");
        }
        self.canvas = Some(canvas);

        // Connect cleanup to tree_exiting so RIDs are freed before node is removed.
        let cleanup = self.base().callable("_release_render_resources");
        self.base_mut().connect("tree_exiting", &cleanup);
    }

    fn process(&mut self, _delta: f64) {
        // Collect input state before stepping the ECS so Lua scripts can query it.
        if let Some(app) = &mut self.app {
            let input = Input::singleton();
            let mut snap = InputSnapshot::new();
            for action in self.input_actions.iter_shared() {
                let action_str = action.to_string();
                snap.set_pressed(&action_str, input.is_action_pressed(&action_str));
                snap.set_just_pressed(&action_str, input.is_action_just_pressed(&action_str));
                snap.set_just_released(&action_str, input.is_action_just_released(&action_str));
            }
            app.set_input(snap);
        }

        // Step ECS (Lua on_frame runs here, enqueuing RenderCommands)
        if let Some(app) = &mut self.app {
            if let Err(e) = app.step() {
                godot_error!("[Evernight] ECS step error: {e:?}");
            }
        }

        // Drain and apply render commands produced this frame
        let Some(rx) = self.receiver.take() else {
            return;
        };
        let mut rs = RenderingServer::singleton();
        while let Ok(cmd) = rx.try_recv() {
            self.apply_command(&mut rs, cmd);
        }
        self.receiver = Some(rx);
    }
}

#[godot_api]
impl EvernightBridge {
    #[func]
    pub fn load_script_file(&mut self, path: GString) {
        let Some(app) = &mut self.app else { return };
        // res:// is a Godot virtual path; resolve it to an OS-level absolute path first.
        let real_path = ProjectSettings::singleton().globalize_path(&path);
        match std::fs::read_to_string(real_path.to_string()) {
            Ok(src) => {
                if let Err(e) = app.load_script(&src) {
                    godot_error!("[Evernight] load_script_file error: {e:?}");
                }
            }
            Err(e) => godot_error!("[Evernight] cannot read file '{path}' ({real_path}): {e}"),
        }
    }

    /// Frees all canvas items and the shared canvas.  Called via signal on
    /// `tree_exiting` so cleanup happens before the node is removed.
    #[func]
    fn _release_render_resources(&mut self) {
        let mut rs = RenderingServer::singleton();
        for (_, rid) in self.rid_map.drain() {
            rs.canvas_item_clear(rid);
            rs.free_rid(rid);
        }
        self.texture_cache.clear();
        if let Some(canvas) = self.canvas.take() {
            rs.free_rid(canvas);
        }
        self.app = None;
    }

    // ── Internal: apply one RenderCommand to RenderingServer ─────────────

    fn apply_command(&mut self, rs: &mut Gd<RenderingServer>, cmd: RenderCommand) {
        match cmd {
            RenderCommand::CreateSprite {
                handle,
                texture_path,
                z_index,
            } => {
                // If a canvas item already exists for this handle (can happen when
                // the ID allocator reuses an entity ID before the old sprite was
                // explicitly destroyed), free the stale item first to avoid leaking
                // RenderingServer resources.
                if let Some(old_item) = self.rid_map.remove(&handle) {
                    rs.canvas_item_clear(old_item);
                    rs.free_rid(old_item);
                }

                let item = rs.canvas_item_create();
                if let Some(canvas) = self.canvas {
                    rs.canvas_item_set_parent(item, canvas);
                }
                rs.canvas_item_set_z_index(item, z_index);

                if let Some(tex) = self.texture_for_path(&texture_path) {
                    let tex_rid = tex.get_rid();
                    let size = tex.get_size();
                    let rect = Rect2::new(Vector2::new(-size.x / 2.0, -size.y / 2.0), size);
                    rs.canvas_item_add_texture_rect(item, rect, tex_rid);
                }

                self.rid_map.insert(handle, item);
            }

            RenderCommand::UpdateTransform {
                handle,
                x,
                y,
                rotation,
                scale_x,
                scale_y,
            } => {
                let Some(&item) = self.rid_map.get(&handle) else {
                    return;
                };
                let cos_r = rotation.cos();
                let sin_r = rotation.sin();
                let t = Transform2D::from_cols(
                    Vector2::new(cos_r * scale_x, sin_r * scale_x),
                    Vector2::new(-sin_r * scale_y, cos_r * scale_y),
                    Vector2::new(x, y),
                );
                rs.canvas_item_set_transform(item, t);
            }

            RenderCommand::SetVisible { handle, visible } => {
                let Some(&item) = self.rid_map.get(&handle) else {
                    return;
                };
                rs.canvas_item_set_visible(item, visible);
            }

            RenderCommand::SetModulate { handle, r, g, b, a } => {
                let Some(&item) = self.rid_map.get(&handle) else {
                    return;
                };
                rs.canvas_item_set_modulate(item, Color::from_rgba(r, g, b, a));
            }

            RenderCommand::DestroySprite { handle } => {
                let Some(item) = self.rid_map.remove(&handle) else {
                    return;
                };
                rs.canvas_item_clear(item);
                rs.free_rid(item);
            }
        }
    }

    /// Returns the cached `Gd<Texture2D>` for `path`, loading it if necessary.
    fn texture_for_path(&mut self, path: &str) -> Option<&Gd<Texture2D>> {
        if !self.texture_cache.contains_key(path) {
            match try_load::<Texture2D>(path) {
                Ok(tex) => {
                    self.texture_cache.insert(path.to_string(), tex);
                }
                Err(e) => {
                    godot_warn!("[Evernight] texture not found: {path}: {e}");
                    return None;
                }
            }
        }
        self.texture_cache.get(path)
    }
}
