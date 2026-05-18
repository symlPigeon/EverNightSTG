/// Core deterministic random-number generator interface.
///
/// Contract:
/// - Same algorithm + same initial state + same call sequence => identical output sequence.
/// - Implementors should avoid platform-dependent behavior.
pub trait DeterministicRng {
    /// Produces the next 64 bits from the generator stream.
    fn next_u64(&mut self) -> u64;

    /// Produces the next 32 bits (lower half of `next_u64`).
    #[inline]
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    /// Produces a float in [0, 1).
    #[inline]
    fn next_f32_01(&mut self) -> f32 {
        // Use the high 24 random bits, matching f32 mantissa precision.
        let v = (self.next_u64() >> 40) as u32;
        (v as f32) * (1.0 / 16_777_216.0)
    }

    /// Produces a float in [0, 1).
    #[inline]
    fn next_f64_01(&mut self) -> f64 {
        // Use the high 53 random bits, matching f64 mantissa precision.
        let v = self.next_u64() >> 11;
        (v as f64) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Uniform integer sampling in half-open interval [min, max_exclusive).
    ///
    /// Panics if `min >= max_exclusive`.
    #[inline]
    fn next_u64_range(&mut self, min: u64, max_exclusive: u64) -> u64 {
        assert!(
            min < max_exclusive,
            "invalid range: min ({min}) must be < max_exclusive ({max_exclusive})"
        );

        // Rejection sampling avoids modulo bias.
        let width = max_exclusive - min;
        let zone = u64::MAX - (u64::MAX % width);

        loop {
            let x = self.next_u64();
            if x < zone {
                return min + (x % width);
            }
        }
    }

    /// Uniform integer sampling in half-open interval [min, max_exclusive).
    #[inline]
    fn next_u32_range(&mut self, min: u32, max_exclusive: u32) -> u32 {
        self.next_u64_range(min as u64, max_exclusive as u64) as u32
    }
}

/// Seed management for deterministic RNGs.
pub trait SeedableRng {
    /// Creates an RNG from a deterministic 64-bit seed.
    fn from_seed(seed: u64) -> Self
    where
        Self: Sized;

    /// Resets internal state from a deterministic 64-bit seed.
    fn reseed(&mut self, seed: u64);
}

/// Snapshot/restore interface for replay, save/load, and rollback.
pub trait StatefulRng {
    type State: Clone;

    /// Returns the full internal state.
    fn state(&self) -> Self::State;

    /// Restores a previously captured state.
    fn set_state(&mut self, state: Self::State);
}

pub mod algorithms;
pub mod factory;

pub use {algorithms::*, factory::*};
