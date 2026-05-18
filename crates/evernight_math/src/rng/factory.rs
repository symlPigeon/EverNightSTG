use crate::{DeterministicRng, SeedableRng, StatefulRng};

pub struct RngFactory;

impl RngFactory {
    pub fn create<T>(&self, seed: u64) -> T
    where
        T: SeedableRng + DeterministicRng + StatefulRng,
    {
        T::from_seed(seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng_factory() {
        use crate::Xoroshiro128ppAlgorithm;
        let factory = RngFactory;
        let mut rng1 = factory.create::<Xoroshiro128ppAlgorithm>(12345);
        let mut rng2 = factory.create::<Xoroshiro128ppAlgorithm>(12345);

        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }
}
