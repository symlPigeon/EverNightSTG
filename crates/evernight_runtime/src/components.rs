use std::collections::BTreeSet;

use evernight_core::{CollisionMask, LayerBit, TagFlags, Tick, impl_component};
use evernight_math::{Angle, Shape2D, Vec2};

#[derive(Debug, Clone, Copy, Default)]
pub struct Transform {
    pub position: Vec2,
    pub rotation: Angle,
}

impl Transform {
    pub fn new(position: Vec2, rotation: Angle) -> Self {
        Transform { position, rotation }
    }

    pub fn identity() -> Self {
        Transform {
            position: Vec2::zero(),
            rotation: Angle(0.0),
        }
    }
}

impl_component!(Transform);

#[derive(Debug, Clone, Copy, Default)]
pub struct Velocity {
    pub linear: Vec2,
    pub angular: Angle,
}

impl Velocity {
    pub fn new(linear: Vec2, angular: Angle) -> Self {
        Velocity { linear, angular }
    }

    pub fn zero() -> Self {
        Velocity {
            linear: Vec2::zero(),
            angular: Angle(0.0),
        }
    }
}

impl_component!(Velocity);

#[derive(Debug, Clone, Copy)]
pub struct Lifetime {
    pub remaining: Tick,
}

impl Lifetime {
    pub fn new(remaining: Tick) -> Self {
        Lifetime { remaining }
    }

    pub fn is_expired(&self) -> bool {
        self.remaining.as_u32() == 0
    }
}

impl Default for Lifetime {
    fn default() -> Self {
        Lifetime {
            remaining: Tick::new(0),
        }
    }
}

impl_component!(Lifetime);

#[derive(Debug, Clone)]
pub struct Hitbox {
    pub shape: Shape2D,
    pub layer: LayerBit,
    pub group: CollisionMask,
    pub hit_once: bool,
}

impl Hitbox {
    pub fn new(shape: Shape2D, layer: LayerBit, group: CollisionMask, hit_once: bool) -> Self {
        Hitbox {
            shape,
            layer,
            group,
            hit_once,
        }
    }
}

impl_component!(Hitbox);

#[derive(Debug, Clone)]
pub struct Hurtbox {
    pub shape: Shape2D,
    pub layer: LayerBit,
}

impl Hurtbox {
    pub fn new(shape: Shape2D, layer: LayerBit) -> Self {
        Hurtbox { shape, layer }
    }
}

impl_component!(Hurtbox);

/// Categorises an entity with fast built-in flags and optional script-defined custom tags.
///
/// - `flags`: O(1) bitwise check against `TagFlags` constants (e.g. `TagFlags::PLAYER`).
/// - `custom`: heap-allocated `BTreeSet<u32>` for script-registered tag IDs.
///   Ordered iteration guarantees determinism. Use a tag registry to map IDs ↔ names.
#[derive(Debug, Clone)]
pub struct Tag {
    pub flags: TagFlags,
    pub custom: BTreeSet<u32>,
}

impl Tag {
    pub fn new(flags: TagFlags) -> Self {
        Tag {
            flags,
            custom: BTreeSet::new(),
        }
    }

    /// Adds a built-in flag.
    pub fn add_flag(&mut self, flag: TagFlags) {
        self.flags |= flag;
    }

    /// Removes a built-in flag.
    pub fn remove_flag(&mut self, flag: TagFlags) {
        self.flags &= !flag;
    }

    /// Returns `true` if the built-in flag is set.
    pub fn has_flag(&self, flag: TagFlags) -> bool {
        self.flags.has(flag)
    }

    /// Inserts a script-defined custom tag ID.
    pub fn add_custom(&mut self, id: u32) {
        self.custom.insert(id);
    }

    /// Removes a script-defined custom tag ID.
    pub fn remove_custom(&mut self, id: u32) {
        self.custom.remove(&id);
    }

    /// Returns `true` if the custom tag ID is present.
    pub fn has_custom(&self, id: u32) -> bool {
        self.custom.contains(&id)
    }
}

impl_component!(Tag);
