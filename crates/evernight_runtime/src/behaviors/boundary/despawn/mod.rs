use evernight_core::{EntityId, impl_component};

use crate::{ComponentStorage, Transform};

/// Marks an entity as despawnable when it leaves a rectangular region centered on the origin.
///
/// The bounds are specified as half-extents: the entity is considered out of bounds when its
/// `Transform.position` leaves `[-half_width, half_width] × [-half_height, half_height]`.
#[derive(Debug, Clone, Copy)]
pub struct DespawnBounds {
	pub half_width: f32,
	pub half_height: f32,
}

impl DespawnBounds {
	pub fn new(half_width: f32, half_height: f32) -> Self {
		Self {
			half_width,
			half_height,
		}
	}
}

impl_component!(DespawnBounds);

/// Returns the entities whose `Transform` has moved outside their declared bounds.
pub fn despawn_out_of_bounds_system(storage: &ComponentStorage) -> Vec<EntityId> {
	let ids: Vec<EntityId> = storage.iter::<DespawnBounds>().map(|(id, _)| id).collect();
	let mut despawned = Vec::new();

	for id in ids {
		let Some(bounds) = storage.get::<DespawnBounds>(id).copied() else {
			continue;
		};
		let Some(tf) = storage.get::<Transform>(id).copied() else {
			continue;
		};

		let out_of_x = tf.position.x < -bounds.half_width || tf.position.x > bounds.half_width;
		let out_of_y = tf.position.y < -bounds.half_height || tf.position.y > bounds.half_height;
		if out_of_x || out_of_y {
			despawned.push(id);
		}
	}

	despawned
}
