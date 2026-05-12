// Detection pipeline (per frame):
//   1. Collect all (entity, Hitbox) and (entity, Hurtbox) from ComponentStorage.
//   2. Apply Transform::apply_to_shape() to convert local-space → world-space.
//   3. Filter pairs by hitbox.group.collides_with(hurtbox.layer)  [broad-phase].
//   4. Run evernight_math::detect() per candidate pair             [narrow-phase].
//   5. Sort confirmed hits by (attacker, defender) for deterministic replay.
//   6. Push EventPayload::Collision events; skip further hits for hit_once hitboxes.
