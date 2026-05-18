use crate::{DeterministicRng, SeedableRng, StatefulRng};

/// A simple, fast, non-cryptographic RNG with good statistical properties.
/// https://prng.di.unimi.it/splitmix64.c
pub struct SplitMix64Algorithm {
    state: u64,
}

impl SplitMix64Algorithm {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl DeterministicRng for SplitMix64Algorithm {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

impl SeedableRng for SplitMix64Algorithm {
    fn from_seed(seed: u64) -> Self {
        Self::new(seed)
    }

    fn reseed(&mut self, seed: u64) {
        self.state = seed;
    }
}

impl StatefulRng for SplitMix64Algorithm {
    type State = u64;

    fn state(&self) -> Self::State {
        self.state
    }

    fn set_state(&mut self, state: Self::State) {
        self.state = state;
    }
}

/// A high-quality, fast RNG suitable for general use, including games and simulations.
/// https://prng.di.unimi.it/xoroshiro128plusplus.c
pub struct Xoroshiro128ppAlgorithm {
    s0: u64,
    s1: u64,
}

impl Xoroshiro128ppAlgorithm {
    pub fn new(seed: u64) -> Self {
        let mut sm64 = SplitMix64Algorithm::new(seed);
        Self {
            s0: sm64.next_u64(),
            s1: sm64.next_u64(),
        }
    }

    pub fn jump(&mut self) {
        // Equivalent to 2^64 calls to next_u64(), for parallel streams.
        let mut s0 = self.s0;
        let mut s1 = self.s1;
        let mut result = 0u64;

        for &jump in &[0x2bd7a6a6e99c2ddcu64, 0x0992ccaf6a6fca05u64] {
            for b in 0..64 {
                if (jump & (1 << b)) != 0 {
                    result ^= s0;
                    result ^= s1;
                }
                s1 ^= s0;
                s0 = s0.rotate_left(24) ^ s1 ^ (s1 << 16);
                s1 = s1.rotate_left(37);
            }
        }

        self.s0 = result;
    }
}

impl DeterministicRng for Xoroshiro128ppAlgorithm {
    fn next_u64(&mut self) -> u64 {
        let s0 = self.s0;
        let mut s1 = self.s1;
        let result = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        s1 ^= s0;
        self.s0 = s0.rotate_left(49) ^ s1 ^ (s1 << 21);
        self.s1 = s1.rotate_left(28);
        result
    }
}

impl SeedableRng for Xoroshiro128ppAlgorithm {
    fn from_seed(seed: u64) -> Self {
        Self::new(seed)
    }

    fn reseed(&mut self, seed: u64) {
        let mut sm64 = SplitMix64Algorithm::new(seed);
        self.s0 = sm64.next_u64();
        self.s1 = sm64.next_u64();
    }
}

impl StatefulRng for Xoroshiro128ppAlgorithm {
    type State = (u64, u64);

    fn state(&self) -> Self::State {
        (self.s0, self.s1)
    }

    fn set_state(&mut self, state: Self::State) {
        self.s0 = state.0;
        self.s1 = state.1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_splitmix64_seed() {
        let mut rng = SplitMix64Algorithm::new(1234567);
        let expected = [
            6457827717110365317u64,
            3203168211198807973u64,
            9817491932198370423u64,
            4593380528125082431u64,
            16408922859458223821u64,
        ];
        for &e in &expected {
            assert_eq!(rng.next_u64(), e);
        }
    }

    #[test]
    fn test_xoroshiro128pp_seed() {
        let mut rng = Xoroshiro128ppAlgorithm::new(123456789);
        let expected = [
            0xc61f6394073ab015u64,
            0xdd60399264041d13u64,
            0x0d97d3cd143d5663u64,
            0xac3f8b8860cb4a64u64,
            0xfc96ab8265118e08u64,
        ];
        for &e in &expected {
            assert_eq!(rng.next_u64(), e);
        }
    }
}
