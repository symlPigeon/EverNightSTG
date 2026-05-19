use evernight_core::{EntityId, impl_component};

use crate::{ComponentStorage, Transform};

/// Marks an entity as wrapping around a rectangular region centered on the origin.
///
/// The bounds are specified as half-extents: when an entity leaves the range
/// `[-half_width, half_width] × [-half_height, half_height]`, it re-enters from the opposite side.
#[derive(Debug, Clone, Copy)]
pub struct WrapBounds {
	pub half_width: f32,
	pub half_height: f32,
}

impl WrapBounds {
	pub fn new(half_width: f32, half_height: f32) -> Self {
		Self {
			half_width,
			half_height,
		}
	}
}

impl_component!(WrapBounds);

fn wrap_value(value: f32, half_extent: f32) -> f32 {
	let span = half_extent * 2.0;
	if span <= 0.0 {
		return value;
	}

	let mut wrapped = value;
	while wrapped < -half_extent {
		wrapped += span;
	}
	while wrapped > half_extent {
		wrapped -= span;
	}
	wrapped
}

/// Wraps entities with `WrapBounds` back into their declared region.
pub fn wrap_system(storage: &mut ComponentStorage) {
	let ids: Vec<EntityId> = storage.iter::<WrapBounds>().map(|(id, _)| id).collect();

	for id in ids {
		let Some(bounds) = storage.get::<WrapBounds>(id).copied() else {
			continue;
		};
		let Some(tf) = storage.get::<Transform>(id).copied() else {
			continue;
		};

		let mut new_tf = tf;
		new_tf.position.x = wrap_value(new_tf.position.x, bounds.half_width);
		new_tf.position.y = wrap_value(new_tf.position.y, bounds.half_height);

		if new_tf.position != tf.position {
			*storage.get_mut::<Transform>(id).unwrap() = new_tf;
		}
	}
}
