use std::sync::mpsc::Sender;

/// Commands produced by Lua scripts on the ECS side and consumed by the Godot
/// main thread to drive `RenderingServer`.
///
/// Keep variants lightweight — they are cloned across the mpsc channel every
/// frame for each entity that calls a render method.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    /// Create a canvas item associated with `handle` and draw `texture_path`
    /// centered on it at the given z-index layer.
    CreateSprite {
        handle: u64,
        texture_path: String,
        z_index: i32,
    },
    /// Update the 2-D transform of the canvas item identified by `handle`.
    UpdateTransform {
        handle: u64,
        x: f32,
        y: f32,
        rotation: f32,
        scale_x: f32,
        scale_y: f32,
    },
    /// Show or hide the canvas item identified by `handle`.
    SetVisible { handle: u64, visible: bool },
    /// Set the RGBA color multiplier of the canvas item identified by `handle`.
    /// Each channel is in the range 0.0 – 1.0.
    SetModulate {
        handle: u64,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    },
    /// Destroy the canvas item and release its associated RID.
    DestroySprite { handle: u64 },
}

/// Sending half of the render-command channel.
///
/// Stored inside [`LuaEngine`] and cloned into each `CtxUserdata` per frame so
/// Lua render methods (`ctx:create_sprite`, etc.) can enqueue commands without
/// blocking.
pub type RenderSender = Sender<RenderCommand>;
