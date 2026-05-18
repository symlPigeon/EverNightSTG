use evernight_core::{CollisionMask, EntityId, EventPayload, Tick};
use evernight_math::{Aabb, Shape2D, Vec2, aabb_of, detect};

use crate::{ComponentStorage, EventBus, Hitbox, Hurtbox, SpatialHashGrid, Transform};

#[cfg(feature = "multithread")]
use rayon::prelude::*;

/// Runs three-stage collision detection between `Hitbox` and `Hurtbox` pairs,
/// emitting `EventPayload::Collision` events for each overlapping pair.
///
/// Pipeline:
/// 1. **Collect** — world-transform all hurtbox shapes, compute their AABBs,
///    and insert them into a per-layer `SpatialHashGrid`.
/// 2. **Broad-phase** — for each hitbox query only the grid cells covered by its
///    AABB and only the layers its `CollisionMask` targets.
/// 3. **Mid-phase** — AABB overlap test against each candidate.
/// 4. **Narrow-phase** — `evernight_math::detect()` on survivors.
/// 5. Sort confirmed hits by `(attacker, defender)` for deterministic replay.
/// 6. Push events; `hit_once` hitboxes stop after their first confirmed hit.
pub fn collision_system(storage: &mut ComponentStorage, event_bus: &mut EventBus, tick: Tick) {
    // Cell size: ~2 × typical shape radius.  64 px suits STG sprites well;
    // small bullets (r ≈ 1-4 px) occupy at most a 2×2 block of cells each.
    const CELL_SIZE: f32 = 64.0;

    // ── Stage 1: collect hurtboxes, compute world-space AABBs ────────────────
    struct HurtEntry {
        entity: EntityId,
        aabb: Aabb,
        shape: Shape2D,
        layer: u32, // LayerBit raw — always a single set bit
    }

    let hurtboxes: Vec<HurtEntry> = storage
        .iter::<Hurtbox>()
        .map(|(id, hb)| {
            let tf = storage.get::<Transform>(id).copied().unwrap_or_default();
            let world_shape = tf.apply_to_shape(&hb.shape);
            let aabb = aabb_of(&world_shape);
            HurtEntry {
                entity: id,
                aabb,
                shape: world_shape,
                layer: hb.layer.as_u32(),
            }
        })
        .collect();

    if hurtboxes.is_empty() {
        return;
    }

    // ── Stage 1 (cont.): build spatial hash grid ─────────────────────────────
    let mut grid = SpatialHashGrid::new(CELL_SIZE);
    for (idx, entry) in hurtboxes.iter().enumerate() {
        grid.insert(entry.layer, entry.aabb, idx);
    }

    // ── Stage 2-4: per-hitbox query → AABB filter → narrow-phase ─────────────
    // Transform is pre-collected so the detection loop needs no further access
    // to `storage` — a prerequisite for the optional `multithread` parallel path.
    let raw_hitboxes: Vec<(EntityId, Shape2D, CollisionMask, bool, Transform)> = storage
        .iter::<Hitbox>()
        .map(|(id, h)| {
            let tf = storage.get::<Transform>(id).copied().unwrap_or_default();
            (id, h.shape.clone(), h.group, h.hit_once, tf)
        })
        .collect();

    // ── Serial detection loop ─────────────────────────────────────────────────
    #[cfg(not(feature = "multithread"))]
    let mut hits: Vec<(EntityId, EntityId, Vec2, Vec2)> = {
        let mut acc: Vec<(EntityId, EntityId, Vec2, Vec2)> = Vec::new();
        let mut candidates: Vec<usize> = Vec::new();
        // Generation-based dedup: O(1) per candidate vs O(k log k) sort+dedup.
        let mut visited: Vec<u32> = vec![0u32; hurtboxes.len()];
        let mut generation: u32 = 0;

        for (att_id, att_shape_local, att_group, att_hit_once, tf) in &raw_hitboxes {
            let att_shape = tf.apply_to_shape(att_shape_local);
            let att_aabb = aabb_of(&att_shape);

            generation = generation.wrapping_add(1);
            candidates.clear();
            grid.query(*att_group, att_aabb, &mut candidates);

            for &hurt_idx in &candidates {
                if visited[hurt_idx] == generation {
                    continue;
                }
                visited[hurt_idx] = generation;
                let hurt = &hurtboxes[hurt_idx];
                if *att_id == hurt.entity {
                    continue;
                }
                if !att_aabb.overlaps(&hurt.aabb) {
                    continue;
                }
                let det = detect(&att_shape, &hurt.shape);
                if det.is_colliding {
                    let contact = det.contact_point.unwrap_or(Vec2::zero());
                    let normal = det.normal.unwrap_or(Vec2::zero());
                    acc.push((*att_id, hurt.entity, contact, normal));
                    if *att_hit_once {
                        break;
                    }
                }
            }
        }
        acc
    };

    // ── Parallel detection loop (feature = "multithread") ────────────────────
    // Uses rayon::fold so each worker thread holds ONE set of reusable buffers
    // (candidates, visited, generation) shared across all hitboxes it processes.
    // This avoids the O(n²) allocation cost of allocating per-hitbox inside
    // flat_map.  fold produces per-thread Vec<hit>; reduce merges them.
    // `hurtboxes` and `grid` are read-only (&-references) and therefore Sync.
    #[cfg(feature = "multithread")]
    let mut hits: Vec<(EntityId, EntityId, Vec2, Vec2)> = {
        let n_hurt = hurtboxes.len();
        raw_hitboxes
            .par_iter()
            .fold(
                // Thread-local state: (hit accumulator, candidate buf, visited buf, generation)
                || {
                    (
                        Vec::<(EntityId, EntityId, Vec2, Vec2)>::new(),
                        Vec::<usize>::new(),
                        vec![0u32; n_hurt],
                        0u32,
                    )
                },
                |(mut acc, mut candidates, mut visited, mut generation),
                 (att_id, att_shape_local, att_group, att_hit_once, tf)| {
                    let att_shape = tf.apply_to_shape(att_shape_local);
                    let att_aabb = aabb_of(&att_shape);

                    generation = generation.wrapping_add(1);
                    candidates.clear();
                    grid.query(*att_group, att_aabb, &mut candidates);

                    for &hurt_idx in &candidates {
                        if visited[hurt_idx] == generation {
                            continue;
                        }
                        visited[hurt_idx] = generation;
                        let hurt = &hurtboxes[hurt_idx];
                        if *att_id == hurt.entity {
                            continue;
                        }
                        if !att_aabb.overlaps(&hurt.aabb) {
                            continue;
                        }
                        let det = detect(&att_shape, &hurt.shape);
                        if det.is_colliding {
                            let contact = det.contact_point.unwrap_or(Vec2::zero());
                            let normal = det.normal.unwrap_or(Vec2::zero());
                            acc.push((*att_id, hurt.entity, contact, normal));
                            if *att_hit_once {
                                break;
                            }
                        }
                    }
                    (acc, candidates, visited, generation)
                },
            )
            // Discard per-thread buffers, keep only the hit vecs, merge.
            .map(|(acc, ..)| acc)
            .reduce(Vec::new, |mut a, b| {
                a.extend(b);
                a
            })
    };

    // ── Stage 5-6: sort for determinism and emit ─────────────────────────────
    hits.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (attacker, defender, contact_point, normal) in hits {
        event_bus.push(EventPayload::Collision {
            attacker,
            defender,
            contact_point,
            normal,
            tick,
        });
    }
}
