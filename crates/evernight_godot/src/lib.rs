use godot::prelude::*;

mod bridge;

struct EvernightExt;

#[gdextension]
unsafe impl ExtensionLibrary for EvernightExt {}
