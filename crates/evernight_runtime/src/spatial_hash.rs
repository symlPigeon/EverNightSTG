use std::collections::HashMap;

use evernight_core::CollisionMask;
use evernight_math::Aabb;

/// A spatial hash grid for broad-phase collision culling in an unbounded 2-D world.
///
/// Hurtboxes are bucketed by *(layer_bit, cell_coordinate)*. When querying for a
/// hitbox with a given `CollisionMask`, only the cells covered by that hitbox's AABB
/// and only the layers whose bit is set in the mask are visited — all other entities
/// are never touched.
///
/// ## Cell size tuning
/// Choose `cell_size ≈ 2 × radius_of_typical_shape`. A cell that is too large
/// accumulates many candidates per query; one that is too small causes shapes to
/// span many cells. For STG enemy bullets (r ≈ 2–8 px) a value of 16–32 is good;
/// for mixed-size layers each caller should pick its own cell size.
pub struct SpatialHashGrid {
    #[allow(dead_code)]
    cell_size: f32,
    inv_cell_size: f32,
    /// `layer_bit_raw` (always a single set bit) → cell → list of hurtbox indices.
    ///
    /// The `Vec<usize>` is cleared but not dropped between frames so that capacity
    /// is reused when the grid is rebuilt with `clear()` + `insert()`.
    buckets: HashMap<u32, HashMap<(i32, i32), Vec<usize>>>,
}

impl SpatialHashGrid {
    /// Creates a new, empty grid with the given cell size.
    pub fn new(cell_size: f32) -> Self {
        debug_assert!(cell_size > 0.0, "cell_size must be positive");
        SpatialHashGrid {
            cell_size,
            inv_cell_size: 1.0 / cell_size,
            buckets: HashMap::new(),
        }
    }

    /// Removes all entries while keeping allocated capacity for reuse.
    pub fn clear(&mut self) {
        for layer_map in self.buckets.values_mut() {
            for cell in layer_map.values_mut() {
                cell.clear();
            }
        }
    }

    /// Inserts hurtbox `idx` (an index into the caller's hurtbox slice) into every
    /// cell covered by `aabb` under the given single-bit `layer`.
    ///
    /// Shapes with infinite AABBs (e.g. `Ray`) are silently skipped.
    pub fn insert(&mut self, layer: u32, aabb: Aabb, idx: usize) {
        if !aabb.min_x.is_finite()
            || !aabb.min_y.is_finite()
            || !aabb.max_x.is_finite()
            || !aabb.max_y.is_finite()
        {
            return;
        }
        let (cx0, cy0, cx1, cy1) = self.cells_for(aabb);
        let layer_map = self.buckets.entry(layer).or_default();
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                layer_map.entry((cx, cy)).or_default().push(idx);
            }
        }
    }

    /// Appends to `out` the indices of all hurtboxes that are in a cell covered by
    /// `aabb` and whose layer bit is set in `mask`.
    ///
    /// The output may contain **duplicates** when a hurtbox spans multiple cells.
    /// The caller is responsible for deduplication before running narrow-phase tests.
    ///
    /// Shapes with infinite AABBs (e.g. `Ray` hitboxes) receive no candidates
    /// (the function returns immediately).
    pub fn query(&self, mask: CollisionMask, aabb: Aabb, out: &mut Vec<usize>) {
        if !aabb.min_x.is_finite()
            || !aabb.min_y.is_finite()
            || !aabb.max_x.is_finite()
            || !aabb.max_y.is_finite()
        {
            return;
        }
        let (cx0, cy0, cx1, cy1) = self.cells_for(aabb);
        // Iterate over each set bit in the mask independently.
        let mut remaining = mask.as_u32();
        while remaining != 0 {
            let bit = remaining & remaining.wrapping_neg(); // isolate lowest set bit
            remaining &= remaining - 1; // clear it
            if let Some(layer_map) = self.buckets.get(&bit) {
                for cy in cy0..=cy1 {
                    for cx in cx0..=cx1 {
                        if let Some(v) = layer_map.get(&(cx, cy)) {
                            out.extend_from_slice(v);
                        }
                    }
                }
            }
        }
    }

    /// Returns the inclusive cell range `(cx0, cy0, cx1, cy1)` covered by `aabb`.
    #[inline]
    fn cells_for(&self, aabb: Aabb) -> (i32, i32, i32, i32) {
        let cx0 = (aabb.min_x * self.inv_cell_size).floor() as i32;
        let cy0 = (aabb.min_y * self.inv_cell_size).floor() as i32;
        let cx1 = (aabb.max_x * self.inv_cell_size).floor() as i32;
        let cy1 = (aabb.max_y * self.inv_cell_size).floor() as i32;
        (cx0, cy0, cx1, cy1)
    }
}
