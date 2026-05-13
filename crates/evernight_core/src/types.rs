/// Unique identifier for an entity in the game world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(u32);

impl EntityId {
    pub fn new(id: u32) -> Self {
        EntityId(id)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Time in ticks since the start of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tick(u32);

impl Tick {
    pub fn new(tick: u32) -> Self {
        Tick(tick)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// A bitmask representing the layers an entity belongs to.
/// Used for collision detection.
///
/// Construct with a zero-based layer index (0–31); internally stores `1 << layer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerBit(u32);

impl LayerBit {
    /// Creates a `LayerBit` from a zero-based layer index (0–31).
    pub fn new(layer: u32) -> Self {
        debug_assert!(layer < 32, "layer index must be 0–31");
        LayerBit(1 << layer)
    }

    /// Returns a `LayerBit` with no layers set.
    pub fn empty() -> Self {
        LayerBit(0)
    }

    /// Checks whether this `LayerBit` overlaps with another.
    pub fn contains(&self, other: LayerBit) -> bool {
        (self.0 & other.0) != 0
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for LayerBit {
    type Output = LayerBit;
    fn bitor(self, other: LayerBit) -> LayerBit {
        LayerBit(self.0 | other.0)
    }
}

/// A bitmask representing the layers an entity can collide with.
/// Used by a `Hitbox` to filter which `Hurtbox` layers it should test against.
///
/// Construct with zero-based layer indices, same as `LayerBit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CollisionMask(u32);

impl CollisionMask {
    /// Creates a `CollisionMask` from a zero-based layer index (0–31).
    pub fn new(layer: u32) -> Self {
        debug_assert!(layer < 32, "layer index must be 0–31");
        CollisionMask(1 << layer)
    }

    /// Returns a `CollisionMask` that matches no layers.
    pub fn empty() -> Self {
        CollisionMask(0)
    }

    /// Returns `true` if this mask overlaps with the given `LayerBit`.
    /// Typical use: `hitbox.mask.collides_with(hurtbox.layer)`
    pub fn collides_with(&self, layer: LayerBit) -> bool {
        (self.0 & layer.as_u32()) != 0
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for CollisionMask {
    type Output = CollisionMask;
    fn bitor(self, other: CollisionMask) -> CollisionMask {
        CollisionMask(self.0 | other.0)
    }
}

/// A typed handle pointing to a resource or registry entry of type `T`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle<T> {
    id: u32,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Handle<T> {
    /// Creates a new `Handle` with the specified ID.
    pub fn new(id: u32) -> Self {
        Handle {
            id,
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns the underlying ID as a reference.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Consumes the handle and returns the underlying ID.
    pub fn as_u32(self) -> u32 {
        self.id
    }
}

/// Errors that can occur in the EverNight engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EverNightError {
    InvalidEntityId(EntityId),
    InvalidHandle,
    ComponentNotFound,
    InvalidState(String),
    AllocatorFull,
    /// Error originating from a scripting backend (e.g. Lua runtime error).
    ScriptError(String),
}

/// A result type for operations in the EverNight engine, using `EverNightError` for error handling.
pub type EverNightResult<T> = Result<T, EverNightError>;

/// A bitmask for built-in entity tags (up to 64 distinct flags).
/// Used for fast O(1) categorisation of entities by the engine.
/// For script-defined dynamic tags, see `Tag::custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TagFlags(pub u64);

impl TagFlags {
    pub const NONE: TagFlags = TagFlags(0);
    pub const PLAYER: TagFlags = TagFlags(1 << 0);
    pub const ENEMY: TagFlags = TagFlags(1 << 1);
    pub const PLAYER_BULLET: TagFlags = TagFlags(1 << 2);
    pub const ENEMY_BULLET: TagFlags = TagFlags(1 << 3);
    pub const PICKUP: TagFlags = TagFlags(1 << 4);
    pub const BOSS: TagFlags = TagFlags(1 << 5);
    pub const INVINCIBLE: TagFlags = TagFlags(1 << 6);
    pub const GRAZE: TagFlags = TagFlags(1 << 7);

    /// Returns `true` if all bits in `other` are set in `self`.
    pub fn has(self, other: TagFlags) -> bool {
        self.0 & other.0 != 0
    }

    /// Returns `true` if no flags are set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for TagFlags {
    type Output = TagFlags;
    fn bitor(self, other: TagFlags) -> TagFlags {
        TagFlags(self.0 | other.0)
    }
}

impl std::ops::BitOrAssign for TagFlags {
    fn bitor_assign(&mut self, other: TagFlags) {
        self.0 |= other.0;
    }
}

impl std::ops::BitAnd for TagFlags {
    type Output = TagFlags;
    fn bitand(self, other: TagFlags) -> TagFlags {
        TagFlags(self.0 & other.0)
    }
}

impl std::ops::BitAndAssign for TagFlags {
    fn bitand_assign(&mut self, other: TagFlags) {
        self.0 &= other.0;
    }
}

impl std::ops::Not for TagFlags {
    type Output = TagFlags;
    fn not(self) -> TagFlags {
        TagFlags(!self.0)
    }
}
