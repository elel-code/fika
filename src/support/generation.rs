#[derive(Debug, Default)]
pub struct GenerationCounter {
    current: u64,
}

impl GenerationCounter {
    pub fn next(&mut self) -> u64 {
        self.current += 1;
        self.current
    }

    pub fn current(&self) -> u64 {
        self.current
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.current == generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidates_stale_results() {
        let mut counter = GenerationCounter::default();
        let first = counter.next();
        let second = counter.next();

        assert!(!counter.is_current(first));
        assert!(counter.is_current(second));
    }

    #[test]
    fn current_reads_without_invalidating_existing_generation() {
        let mut counter = GenerationCounter::default();
        let generation = counter.next();

        assert_eq!(counter.current(), generation);
        assert!(counter.is_current(generation));
    }
}
