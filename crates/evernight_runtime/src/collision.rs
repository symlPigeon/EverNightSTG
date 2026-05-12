// Broad-phase and integration helpers for collision detection.
//
// Responsibility split:
//   - Broad-phase (mask filtering): collision_system in systems.rs
//   - Narrow-phase (shape overlap):  evernight_math::detect()
//
// Detection pipeline (per frame):
//   1. Collect all (entity, Hitbox) and (entity, Hurtbox) from ComponentStorage.
//   2. Filter pairs by hitbox.group.collides_with(hurtbox.layer).
//   3. Run detect(hitbox.shape, hurtbox.shape) for each candidate pair.
//   4. Sort confirmed hits by (attacker, defender) for deterministic replay.
//   5. Push EventPayload::Collision events; skip further hits for hit_once hitboxes.
