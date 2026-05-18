use evernight_benchmark::{
    AppFrameProfile, BenchResult, ProfileAccumulator, WorldFrameProfile, bench,
    print_profile_report, print_results,
};
use evernight_core::{CollisionMask, EntityId, LayerBit, SpawnRequest, impl_component};
use evernight_lua::LuaEngine;
use evernight_math::{Angle, Circle, Shape2D, Vec2};
use evernight_runtime::{FixedStep, Hitbox, Hurtbox, Scheduler, Transform, Velocity, World};
use evernight_script::{App, ScriptEngine};

// ── Setup helpers ─────────────────────────────────────────────────────────────

fn make_world() -> World {
    World::new(FixedStep::new_60hz())
}

fn no_hooks() -> Scheduler {
    Scheduler::new()
}

fn populate_moving(world: &mut World, n: u32) {
    let mut sched = no_hooks();
    for i in 0..n {
        let e = world.spawn_entity(SpawnRequest::new()).unwrap();
        let x = (i % 100) as f32 * 2.0;
        let y = (i / 100) as f32 * 2.0;
        world
            .add_component(e, Transform::new(Vec2::new(x, y), Angle(0.0)))
            .unwrap();
        world
            .add_component(
                e,
                Velocity {
                    linear: Vec2::new(0.1, 0.0),
                    angular: Angle(0.0),
                },
            )
            .unwrap();
    }
    world.step(&mut sched).unwrap();
}

fn populate_collision(world: &mut World, n: u32, cluster: bool) {
    let mut sched = no_hooks();
    for i in 0..n {
        let e = world.spawn_entity(SpawnRequest::new()).unwrap();
        let pos = if cluster {
            Vec2::zero()
        } else {
            Vec2::new(i as f32 * 100.0, 0.0)
        };
        world
            .add_component(e, Transform::new(pos, Angle(0.0)))
            .unwrap();
        world
            .add_component(
                e,
                Hitbox::new(
                    Shape2D::Circle(Circle {
                        center: Vec2::zero(),
                        radius: 1.0,
                    }),
                    LayerBit::new(0),
                    CollisionMask::new(0),
                    false,
                ),
            )
            .unwrap();
        world
            .add_component(
                e,
                Hurtbox::new(
                    Shape2D::Circle(Circle {
                        center: Vec2::zero(),
                        radius: 1.0,
                    }),
                    LayerBit::new(0),
                ),
            )
            .unwrap();
    }
    world.step(&mut sched).unwrap();
}

// ── World step benchmarks ─────────────────────────────────────────────────────

fn bm_world_step_empty() -> BenchResult {
    let mut world = make_world();
    let mut sched = no_hooks();
    bench("world_step_empty", 50, 1000, || {
        world.step(&mut sched).unwrap();
    })
}

fn bm_world_step_1k_entities() -> BenchResult {
    let mut world = make_world();
    populate_moving(&mut world, 1_000);
    let mut sched = no_hooks();
    bench("world_step_1k_entities", 50, 500, || {
        world.step(&mut sched).unwrap();
    })
}

fn bm_world_step_10k_entities() -> BenchResult {
    let mut world = make_world();
    populate_moving(&mut world, 10_000);
    let mut sched = no_hooks();
    bench("world_step_10k_entities", 20, 200, || {
        world.step(&mut sched).unwrap();
    })
}

// ── Spawn benchmarks ──────────────────────────────────────────────────────────

fn bm_spawn_storm_1k() -> BenchResult {
    bench("spawn_storm_1k", 10, 200, || {
        let mut world = make_world();
        let mut sched = no_hooks();
        for _ in 0..1_000 {
            world.spawn_entity(SpawnRequest::new()).unwrap();
        }
        world.step(&mut sched).unwrap();
    })
}

fn bm_spawn_storm_10k() -> BenchResult {
    bench("spawn_storm_10k", 5, 50, || {
        let mut world = make_world();
        let mut sched = no_hooks();
        for _ in 0..10_000 {
            world.spawn_entity(SpawnRequest::new()).unwrap();
        }
        world.step(&mut sched).unwrap();
    })
}

// ── Lua benchmarks ────────────────────────────────────────────────────────────

fn bm_lua_on_frame_noop() -> BenchResult {
    let mut engine = LuaEngine::new().unwrap();
    engine.load("function on_frame(ctx) end").unwrap();
    let mut app = App::new(FixedStep::new_60hz());
    app.set_script_engine(Box::new(engine));
    bench("lua_on_frame_noop", 50, 1000, || {
        app.step().unwrap();
    })
}

fn make_lua_rw_app(n: u32) -> App {
    #[derive(Clone)]
    struct Pos {
        x: f32,
        y: f32,
    }
    impl_component!(Pos);

    let mut engine = LuaEngine::new().unwrap();
    engine.register_component::<Pos, _, _>(
        "Pos",
        |p, lua| {
            let t = lua.create_table()?;
            t.set("x", p.x)?;
            t.set("y", p.y)?;
            Ok(t)
        },
        |t| {
            Ok(Pos {
                x: t.get("x")?,
                y: t.get("y")?,
            })
        },
    );
    engine
        .load(
            r#"
        function on_frame(ctx)
            for _, id in ipairs(_G.entities) do
                local p = ctx:get_component(id, "Pos")
                if p then
                    p.x = p.x + 1.0
                    ctx:set_component(id, "Pos", p)
                end
            end
        end
    "#,
        )
        .unwrap();

    let mut app = App::new(FixedStep::new_60hz());
    let mut ids: Vec<u32> = Vec::new();
    for _ in 0..n {
        let e = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        ids.push(e.as_u32());
    }
    app.step().unwrap();
    for &id in &ids {
        app.world_mut()
            .add_component_boxed(EntityId::new(id), Box::new(Pos { x: 0.0, y: 0.0 }))
            .unwrap();
    }
    app.step().unwrap();
    app.set_script_engine(Box::new(engine));

    let ids_lua = format!(
        "_G.entities = {{ {} }}",
        ids.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    app.load_script(&ids_lua).unwrap();
    app
}

fn bm_lua_component_rw_1k() -> BenchResult {
    let mut app = make_lua_rw_app(1_000);
    bench("lua_component_rw_1k", 20, 100, || {
        app.step().unwrap();
    })
}

fn bm_lua_component_rw_10k() -> BenchResult {
    let mut app = make_lua_rw_app(10_000);
    bench("lua_component_rw_10k", 5, 30, || {
        app.step().unwrap();
    })
}

// ── Collision benchmarks ──────────────────────────────────────────────────────
// Collision is O(n*m): n=hitboxes, m=hurtboxes.
// No-overlap tests pair enumeration; all-overlap tests event emission at scale.

fn bm_collision_no_overlap_1k() -> BenchResult {
    let mut world = make_world();
    populate_collision(&mut world, 1_000, false);
    let mut sched = no_hooks();
    bench("collision_no_overlap_1k", 10, 100, || {
        world.step(&mut sched).unwrap();
    })
}

// O(n^2): 5k is ~25x heavier than 1k. Use few iterations.
fn bm_collision_no_overlap_5k() -> BenchResult {
    let mut world = make_world();
    populate_collision(&mut world, 5_000, false);
    let mut sched = no_hooks();
    bench("collision_no_overlap_5k", 3, 10, || {
        world.step(&mut sched).unwrap();
    })
}

fn bm_collision_all_overlap_100() -> BenchResult {
    let mut world = make_world();
    populate_collision(&mut world, 100, true);
    let mut sched = no_hooks();
    bench("collision_all_overlap_100", 10, 200, || {
        world.step(&mut sched).unwrap();
    })
}

// 500*499/2 ~= 125k pairs -> ~6 ms/iter
fn bm_collision_all_overlap_500() -> BenchResult {
    let mut world = make_world();
    populate_collision(&mut world, 500, true);
    let mut sched = no_hooks();
    bench("collision_all_overlap_500", 5, 50, || {
        world.step(&mut sched).unwrap();
    })
}

// 100 entities all clustered -> ~4950 Collision events read by Lua each frame.
fn bm_collision_event_dispatch_lua_100() -> BenchResult {
    let mut engine = LuaEngine::new().unwrap();
    engine
        .load(
            r#"
        _G.hit_count = 0
        function on_frame(ctx)
            for _, e in ipairs(ctx:events()) do
                if e.type == "Collision" then
                    _G.hit_count = _G.hit_count + 1
                end
            end
        end
    "#,
        )
        .unwrap();

    let mut app = App::new(FixedStep::new_60hz());
    for _ in 0..100 {
        let e = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(e, Transform::new(Vec2::zero(), Angle(0.0)))
            .unwrap();
        app.world_mut()
            .add_component(
                e,
                Hitbox::new(
                    Shape2D::Circle(Circle {
                        center: Vec2::zero(),
                        radius: 1.0,
                    }),
                    LayerBit::new(0),
                    CollisionMask::new(0),
                    false,
                ),
            )
            .unwrap();
        app.world_mut()
            .add_component(
                e,
                Hurtbox::new(
                    Shape2D::Circle(Circle {
                        center: Vec2::zero(),
                        radius: 1.0,
                    }),
                    LayerBit::new(0),
                ),
            )
            .unwrap();
    }
    app.step().unwrap();
    app.set_script_engine(Box::new(engine));

    bench("collision_event_dispatch_lua_100ent", 10, 100, || {
        app.step().unwrap();
    })
}

// ── P2/P3 feature benchmarks ──────────────────────────────────────────────────

/// Cost of `ctx:iter_entities("Transform")` on 1 000 entities.
/// Exercises ComponentStorage::iter_ids_dyn + Lua table construction.
fn bm_lua_iter_entities_transform_1k() -> BenchResult {
    let mut engine = LuaEngine::new().unwrap();
    engine
        .load(
            r#"
        function on_frame(ctx)
            local ids = ctx:iter_entities("Transform")
            local _ = #ids  -- prevent dead-code elimination
        end
    "#,
        )
        .unwrap();

    let mut app = App::new(FixedStep::new_60hz());
    for i in 0..1_000u32 {
        let e = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        let x = (i % 100) as f32 * 2.0;
        let y = (i / 100) as f32 * 2.0;
        app.world_mut()
            .add_component(e, Transform::new(Vec2::new(x, y), Angle(0.0)))
            .unwrap();
    }
    app.set_script_engine(Box::new(engine));

    bench("lua_iter_entities_transform_1k", 20, 200, || {
        app.step().unwrap();
    })
}

/// Same as above but for 10 000 entities.
fn bm_lua_iter_entities_transform_10k() -> BenchResult {
    let mut engine = LuaEngine::new().unwrap();
    engine
        .load(
            r#"
        function on_frame(ctx)
            local ids = ctx:iter_entities("Transform")
            local _ = #ids
        end
    "#,
        )
        .unwrap();

    let mut app = App::new(FixedStep::new_60hz());
    for i in 0..10_000u32 {
        let e = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        let x = (i % 100) as f32 * 2.0;
        let y = (i / 100) as f32 * 2.0;
        app.world_mut()
            .add_component(e, Transform::new(Vec2::new(x, y), Angle(0.0)))
            .unwrap();
    }
    app.set_script_engine(Box::new(engine));

    bench("lua_iter_entities_transform_10k", 5, 50, || {
        app.step().unwrap();
    })
}

/// 100 overlapping entities; collision events dispatched to `on_collision(ctx,…)`
/// instead of scanned via `ctx:events()`.  Direct comparison with
/// `collision_event_dispatch_lua_100ent` reveals per-call vs. batch-read overhead.
fn bm_lua_on_collision_callback_100ent() -> BenchResult {
    let mut engine = LuaEngine::new().unwrap();
    engine
        .load(
            r#"
        _G.hit_count = 0
        function on_collision(ctx, att, def, cx, cy, nx, ny)
            _G.hit_count = _G.hit_count + 1
        end
    "#,
        )
        .unwrap();

    let mut app = App::new(FixedStep::new_60hz());
    for _ in 0..100 {
        let e = app.world_mut().spawn_entity(SpawnRequest::new()).unwrap();
        app.world_mut()
            .add_component(e, Transform::new(Vec2::zero(), Angle(0.0)))
            .unwrap();
        app.world_mut()
            .add_component(
                e,
                Hitbox::new(
                    Shape2D::Circle(Circle {
                        center: Vec2::zero(),
                        radius: 1.0,
                    }),
                    LayerBit::new(0),
                    CollisionMask::new(0),
                    false,
                ),
            )
            .unwrap();
        app.world_mut()
            .add_component(
                e,
                Hurtbox::new(
                    Shape2D::Circle(Circle {
                        center: Vec2::zero(),
                        radius: 1.0,
                    }),
                    LayerBit::new(0),
                ),
            )
            .unwrap();
    }
    app.step().unwrap();
    app.set_script_engine(Box::new(engine));

    bench("lua_on_collision_callback_100ent", 10, 100, || {
        app.step().unwrap();
    })
}

/// Cost of calling `require("utils")` every frame when the module is already
/// cached in `_LOADED`.  Should be essentially a single Lua table lookup.
fn bm_lua_require_cached() -> BenchResult {
    let mut engine = LuaEngine::new().unwrap();
    engine
        .add_module(
            "utils",
            r#"local M = {}
        function M.noop() end
        return M"#,
        )
        .unwrap();
    engine
        .load(
            r#"
        function on_frame(ctx)
            local u = require("utils")
            u.noop()
        end
    "#,
        )
        .unwrap();

    let mut app = App::new(FixedStep::new_60hz());
    app.set_script_engine(Box::new(engine));
    app.step().unwrap(); // first call: executes module source, populates _LOADED

    bench("lua_require_cached", 20, 1000, || {
        app.step().unwrap();
    })
}

// ── Phase-breakdown profile benchmarks ───────────────────────────────────────

fn profile_world(name: &str, world: &mut World, sched: &mut Scheduler, warmup: u32, frames: u32) {
    for _ in 0..warmup {
        world.step_profiled(sched).unwrap();
    }
    let mut acc: ProfileAccumulator<WorldFrameProfile> = ProfileAccumulator::new();
    for _ in 0..frames {
        let (_, p) = world.step_profiled(sched).unwrap();
        acc.push(p);
    }
    print_profile_report(name, &acc);
}

fn profile_app(name: &str, app: &mut App, warmup: u32, frames: u32) {
    for _ in 0..warmup {
        app.step_profiled().unwrap();
    }
    let mut acc: ProfileAccumulator<AppFrameProfile> = ProfileAccumulator::new();
    for _ in 0..frames {
        let (_, p) = app.step_profiled().unwrap();
        acc.push(p);
    }
    print_profile_report(name, &acc);
}

fn run_profile_benchmarks() {
    println!("=== Phase breakdown profiles ===\n");

    // World: empty
    {
        let mut world = make_world();
        let mut sched = no_hooks();
        profile_world("world_empty", &mut world, &mut sched, 50, 500);
    }

    // World: 1k moving entities
    {
        let mut world = make_world();
        populate_moving(&mut world, 1_000);
        let mut sched = no_hooks();
        profile_world("world_1k_movement", &mut world, &mut sched, 50, 500);
    }

    // World: 100 all-overlapping entities (collision heavy)
    {
        let mut world = make_world();
        populate_collision(&mut world, 100, true);
        let mut sched = no_hooks();
        profile_world(
            "world_100_all_overlap_collision",
            &mut world,
            &mut sched,
            20,
            200,
        );
    }

    // App: Lua noop
    {
        let mut engine = LuaEngine::new().unwrap();
        engine.load("function on_frame(ctx) end").unwrap();
        let mut app = App::new(FixedStep::new_60hz());
        app.set_script_engine(Box::new(engine));
        profile_app("app_lua_noop", &mut app, 50, 500);
    }

    // App: Lua component R/W on 1k entities
    {
        let mut app = make_lua_rw_app(1_000);
        profile_app("app_lua_component_rw_1k", &mut app, 20, 200);
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

type BenchFn = fn() -> BenchResult;

/// All registered benchmarks in declaration order.
/// Name must match the string passed to `bench(...)` inside each function.
const ALL: &[(&str, BenchFn)] = &[
    ("world_step_empty", bm_world_step_empty),
    ("world_step_1k_entities", bm_world_step_1k_entities),
    ("world_step_10k_entities", bm_world_step_10k_entities),
    ("spawn_storm_1k", bm_spawn_storm_1k),
    ("spawn_storm_10k", bm_spawn_storm_10k),
    ("lua_on_frame_noop", bm_lua_on_frame_noop),
    ("lua_component_rw_1k", bm_lua_component_rw_1k),
    ("lua_component_rw_10k", bm_lua_component_rw_10k),
    ("collision_no_overlap_1k", bm_collision_no_overlap_1k),
    ("collision_no_overlap_5k", bm_collision_no_overlap_5k),
    ("collision_all_overlap_100", bm_collision_all_overlap_100),
    ("collision_all_overlap_500", bm_collision_all_overlap_500),
    (
        "collision_event_dispatch_lua_100ent",
        bm_collision_event_dispatch_lua_100,
    ),
    (
        "lua_iter_entities_transform_1k",
        bm_lua_iter_entities_transform_1k,
    ),
    (
        "lua_iter_entities_transform_10k",
        bm_lua_iter_entities_transform_10k,
    ),
    (
        "lua_on_collision_callback_100ent",
        bm_lua_on_collision_callback_100ent,
    ),
    ("lua_require_cached", bm_lua_require_cached),
];

fn main() {
    let filter = std::env::args().nth(1);

    println!("Evernight engine -- baseline benchmarks");
    println!("CPU: {}\n", cpu_name());

    // `benchmark profile` → only run phase-breakdown profiles
    if filter.as_deref() == Some("profile") {
        run_profile_benchmarks();
        return;
    }

    if let Some(ref pat) = filter {
        // ── Filtered run ──────────────────────────────────────────────────────
        let matched: Vec<BenchResult> = ALL
            .iter()
            .filter(|(name, _)| name.contains(pat.as_str()))
            .map(|(_, f)| f())
            .collect();

        if matched.is_empty() {
            eprintln!("no benchmark name contains {:?}", pat);
            eprintln!("available:");
            for (name, _) in ALL {
                eprintln!("  {name}");
            }
            std::process::exit(1);
        }
        print_results(&matched);
    } else {
        // ── Full run (grouped) ────────────────────────────────────────────────
        println!("=== World step ===");
        print_results(&[
            bm_world_step_empty(),
            bm_world_step_1k_entities(),
            bm_world_step_10k_entities(),
        ]);

        println!("=== Spawn ===");
        print_results(&[bm_spawn_storm_1k(), bm_spawn_storm_10k()]);

        println!("=== Lua scripting ===");
        print_results(&[
            bm_lua_on_frame_noop(),
            bm_lua_component_rw_1k(),
            bm_lua_component_rw_10k(),
        ]);

        println!("=== Collision ===");
        print_results(&[
            bm_collision_no_overlap_1k(),
            bm_collision_no_overlap_5k(),
            bm_collision_all_overlap_100(),
            bm_collision_all_overlap_500(),
            bm_collision_event_dispatch_lua_100(),
        ]);

        println!("=== P2/P3 feature benchmarks ===");
        print_results(&[
            bm_lua_iter_entities_transform_1k(),
            bm_lua_iter_entities_transform_10k(),
            bm_lua_on_collision_callback_100ent(),
            bm_lua_require_cached(),
        ]);
    }
}

fn cpu_name() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in s.lines() {
                if line.starts_with("model name") {
                    if let Some(v) = line.split(':').nth(1) {
                        return v.trim().to_string();
                    }
                }
            }
        }
    }
    std::env::consts::ARCH.to_string()
}
