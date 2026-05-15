/// Lua ↔ Rust conversion helpers for built-in engine components and shapes.
///
/// `shape_to_table` / `table_to_shape` handle the full `Shape2D` union.
/// Per-component pairs (`transform_to_table` / `table_to_transform`, etc.) are
/// consumed by `LuaEngine::register_builtins()` which calls them at construction
/// time so every `LuaEngine` automatically exposes the core component set.
use evernight_core::{CollisionMask, LayerBit, TagFlags, Tick};
use evernight_math::{
    Angle, Capsule, Circle, Ellipse, Line, Polygon, Ray, Rectangle, Shape2D, Triangle, Vec2,
};
use evernight_runtime::{Hitbox, Hurtbox, Lifetime, Tag, Transform, Velocity, ElasticCollision, Bounded};
use mlua::{Lua, Table};

// ── Shape2D ──────────────────────────────────────────────────────────────────
//
// Lua table schema (discriminated by the "type" string field):
//
//   Circle    : { type="circle",   cx, cy, r }
//   Rectangle : { type="rect",     x, y, w, h, rotation? }
//   Triangle  : { type="triangle", ax, ay, bx, by, tx, ty }
//   Ellipse   : { type="ellipse",  cx, cy, rx, ry, angle? }
//   Capsule   : { type="capsule",  x1, y1, x2, y2, r }
//   Polygon   : { type="polygon",  vertices = {{x,y}, ...} }
//   Line      : { type="line",     x1, y1, x2, y2 }
//   Ray       : { type="ray",      ox, oy, dx, dy }

pub fn shape_to_table(shape: &Shape2D, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    match shape {
        Shape2D::Circle(c) => {
            t.set("type", "circle")?;
            t.set("cx", c.center.x)?;
            t.set("cy", c.center.y)?;
            t.set("r", c.radius)?;
        }
        Shape2D::Rectangle(r) => {
            t.set("type", "rect")?;
            t.set("x", r.position.x)?;
            t.set("y", r.position.y)?;
            t.set("w", r.size.x)?;
            t.set("h", r.size.y)?;
            t.set("rotation", r.rotation.0)?;
        }
        Shape2D::Triangle(tri) => {
            // "tx/ty" for vertex c to avoid shadowing the table variable
            t.set("type", "triangle")?;
            t.set("ax", tri.a.x)?;
            t.set("ay", tri.a.y)?;
            t.set("bx", tri.b.x)?;
            t.set("by", tri.b.y)?;
            t.set("tx", tri.c.x)?;
            t.set("ty", tri.c.y)?;
        }
        Shape2D::Ellipse(e) => {
            t.set("type", "ellipse")?;
            t.set("cx", e.center.x)?;
            t.set("cy", e.center.y)?;
            t.set("rx", e.radii.0)?;
            t.set("ry", e.radii.1)?;
            t.set("angle", e.major_axis_angle)?;
        }
        Shape2D::Capsule(c) => {
            t.set("type", "capsule")?;
            t.set("x1", c.start.x)?;
            t.set("y1", c.start.y)?;
            t.set("x2", c.end.x)?;
            t.set("y2", c.end.y)?;
            t.set("r", c.radius)?;
        }
        Shape2D::Polygon(p) => {
            t.set("type", "polygon")?;
            let verts = lua.create_table()?;
            for (i, v) in p.vertices.iter().enumerate() {
                let vt = lua.create_table()?;
                vt.set("x", v.x)?;
                vt.set("y", v.y)?;
                verts.set(i + 1, vt)?;
            }
            t.set("vertices", verts)?;
        }
        Shape2D::Line(l) => {
            t.set("type", "line")?;
            t.set("x1", l.start.x)?;
            t.set("y1", l.start.y)?;
            t.set("x2", l.end.x)?;
            t.set("y2", l.end.y)?;
        }
        Shape2D::Ray(r) => {
            t.set("type", "ray")?;
            t.set("ox", r.origin.x)?;
            t.set("oy", r.origin.y)?;
            t.set("dx", r.direction.x)?;
            t.set("dy", r.direction.y)?;
        }
    }
    Ok(t)
}

pub fn table_to_shape(t: &Table) -> mlua::Result<Shape2D> {
    let kind: String = t.get("type")?;
    match kind.as_str() {
        "circle" => Ok(Shape2D::Circle(Circle {
            center: Vec2::new(t.get::<f32>("cx")?, t.get::<f32>("cy")?),
            radius: t.get("r")?,
        })),
        "rect" => Ok(Shape2D::Rectangle(Rectangle {
            position: Vec2::new(t.get::<f32>("x")?, t.get::<f32>("y")?),
            size: Vec2::new(t.get::<f32>("w")?, t.get::<f32>("h")?),
            rotation: Angle(t.get::<f32>("rotation").unwrap_or(0.0)),
        })),
        "triangle" => Ok(Shape2D::Triangle(Triangle {
            a: Vec2::new(t.get("ax")?, t.get("ay")?),
            b: Vec2::new(t.get("bx")?, t.get("by")?),
            c: Vec2::new(t.get("tx")?, t.get("ty")?),
        })),
        "ellipse" => Ok(Shape2D::Ellipse(Ellipse {
            center: Vec2::new(t.get("cx")?, t.get("cy")?),
            radii: (t.get("rx")?, t.get("ry")?),
            major_axis_angle: t.get::<f32>("angle").unwrap_or(0.0),
        })),
        "capsule" => Ok(Shape2D::Capsule(Capsule {
            start: Vec2::new(t.get("x1")?, t.get("y1")?),
            end: Vec2::new(t.get("x2")?, t.get("y2")?),
            radius: t.get("r")?,
        })),
        "polygon" => {
            let verts_table: Table = t.get("vertices")?;
            let mut vertices = Vec::new();
            for pair in verts_table.sequence_values::<Table>() {
                let vt = pair?;
                vertices.push(Vec2::new(vt.get("x")?, vt.get("y")?));
            }
            Ok(Shape2D::Polygon(Polygon { vertices }))
        }
        "line" => Ok(Shape2D::Line(Line {
            start: Vec2::new(t.get("x1")?, t.get("y1")?),
            end: Vec2::new(t.get("x2")?, t.get("y2")?),
        })),
        "ray" => Ok(Shape2D::Ray(Ray {
            origin: Vec2::new(t.get("ox")?, t.get("oy")?),
            direction: Vec2::new(t.get("dx")?, t.get("dy")?),
        })),
        other => Err(mlua::Error::RuntimeError(format!(
            "unknown shape type: '{other}'"
        ))),
    }
}

// ── Transform ─────────────────────────────────────────────────────────────────
// { x, y, rotation }  — rotation in radians (f32)

pub fn transform_to_table(tf: &Transform, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("x", tf.position.x)?;
    t.set("y", tf.position.y)?;
    t.set("rotation", tf.rotation.0)?;
    Ok(t)
}

pub fn table_to_transform(t: &Table) -> mlua::Result<Transform> {
    Ok(Transform::new(
        Vec2::new(
            t.get::<f32>("x").unwrap_or(0.0),
            t.get::<f32>("y").unwrap_or(0.0),
        ),
        Angle(t.get::<f32>("rotation").unwrap_or(0.0)),
    ))
}

// ── Velocity ──────────────────────────────────────────────────────────────────
// { vx, vy, angular }  — angular in radians/tick (f32)

pub fn velocity_to_table(v: &Velocity, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("vx", v.linear.x)?;
    t.set("vy", v.linear.y)?;
    t.set("angular", v.angular.0)?;
    Ok(t)
}

pub fn table_to_velocity(t: &Table) -> mlua::Result<Velocity> {
    Ok(Velocity::new(
        Vec2::new(
            t.get::<f32>("vx").unwrap_or(0.0),
            t.get::<f32>("vy").unwrap_or(0.0),
        ),
        Angle(t.get::<f32>("angular").unwrap_or(0.0)),
    ))
}

// ── Tag ───────────────────────────────────────────────────────────────────────
// { flags: integer (u64 bitmask), custom: [u32, ...] }

pub fn tag_to_table(tag: &Tag, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("flags", tag.flags.0)?;
    let custom = lua.create_table()?;
    for (i, &id) in tag.custom.iter().enumerate() {
        custom.set(i + 1, id)?;
    }
    t.set("custom", custom)?;
    Ok(t)
}

pub fn table_to_tag(t: &Table) -> mlua::Result<Tag> {
    let flags_raw: u64 = t.get::<u64>("flags").unwrap_or(0);
    let mut tag = Tag::new(TagFlags(flags_raw));
    if let Ok(custom) = t.get::<Table>("custom") {
        for id in custom.sequence_values::<u32>() {
            tag.custom.insert(id?);
        }
    }
    Ok(tag)
}

// ── Lifetime ──────────────────────────────────────────────────────────────────
// { remaining: integer (ticks) }

pub fn lifetime_to_table(lt: &Lifetime, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("remaining", lt.remaining.as_u32())?;
    Ok(t)
}

pub fn table_to_lifetime(t: &Table) -> mlua::Result<Lifetime> {
    let remaining: u32 = t.get("remaining").unwrap_or(0);
    Ok(Lifetime::new(Tick::new(remaining)))
}

// ── Hitbox ────────────────────────────────────────────────────────────────────
// { shape: {type,...}, layer: 0-31, group: u32 bitmask, hit_once: bool }

pub fn hitbox_to_table(hb: &Hitbox, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("shape", shape_to_table(&hb.shape, lua)?)?;
    // LayerBit is a power-of-two; trailing_zeros recovers the 0-based index.
    t.set("layer", hb.layer.as_u32().trailing_zeros())?;
    t.set("group", hb.group.as_u32())?;
    t.set("hit_once", hb.hit_once)?;
    Ok(t)
}

pub fn table_to_hitbox(t: &Table) -> mlua::Result<Hitbox> {
    let shape_table: Table = t.get("shape")?;
    let shape = table_to_shape(&shape_table)?;
    let layer_idx: u32 = t.get::<u32>("layer").unwrap_or(0).min(31);
    let group_raw: u32 = t.get("group").unwrap_or(0);
    let hit_once: bool = t.get("hit_once").unwrap_or(false);
    Ok(Hitbox::new(
        shape,
        LayerBit::new(layer_idx),
        CollisionMask::from_raw(group_raw),
        hit_once,
    ))
}

// ── Hurtbox ───────────────────────────────────────────────────────────────────
// { shape: {type,...}, layer: 0-31 }

pub fn hurtbox_to_table(hb: &Hurtbox, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("shape", shape_to_table(&hb.shape, lua)?)?;
    t.set("layer", hb.layer.as_u32().trailing_zeros())?;
    Ok(t)
}

pub fn table_to_hurtbox(t: &Table) -> mlua::Result<Hurtbox> {
    let shape_table: Table = t.get("shape")?;
    let shape = table_to_shape(&shape_table)?;
    let layer_idx: u32 = t.get::<u32>("layer").unwrap_or(0).min(31);
    Ok(Hurtbox::new(shape, LayerBit::new(layer_idx)))
}

// ── ElasticCollision ─────────────────────────────────────────────────────────
// { restitution: number }

pub fn elastic_collision_to_table(ec: &ElasticCollision, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("restitution", ec.restitution)?;
    Ok(t)
}

pub fn table_to_elastic_collision(t: &Table) -> mlua::Result<ElasticCollision> {
    let restitution: f32 = t.get::<f32>("restitution").unwrap_or(1.0);
    Ok(ElasticCollision::new(restitution))
}

// ── Bounded ───────────────────────────────────────────────────────────────────
// { half_width: number, half_height: number }

pub fn bounded_to_table(b: &Bounded, lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("half_width", b.half_width)?;
    t.set("half_height", b.half_height)?;
    Ok(t)
}

pub fn table_to_bounded(t: &Table) -> mlua::Result<Bounded> {
    let hw: f32 = t.get::<f32>("half_width").unwrap_or(0.0);
    let hh: f32 = t.get::<f32>("half_height").unwrap_or(0.0);
    Ok(Bounded::new(hw, hh))
}

// ── Tag flag name → TagFlags ──────────────────────────────────────────────────
pub fn flag_name_to_flags(name: &str) -> mlua::Result<TagFlags> {
    match name {
        "player" => Ok(TagFlags::PLAYER),
        "enemy" => Ok(TagFlags::ENEMY),
        "player_bullet" => Ok(TagFlags::PLAYER_BULLET),
        "enemy_bullet" => Ok(TagFlags::ENEMY_BULLET),
        "pickup" => Ok(TagFlags::PICKUP),
        "boss" => Ok(TagFlags::BOSS),
        "invincible" => Ok(TagFlags::INVINCIBLE),
        "graze" => Ok(TagFlags::GRAZE),
        other => Err(mlua::Error::RuntimeError(format!(
            "unknown tag flag '{other}'; valid names: player, enemy, player_bullet, enemy_bullet, pickup, boss, invincible, graze"
        ))),
    }
}
