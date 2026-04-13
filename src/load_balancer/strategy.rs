//! Load balancing selection strategies.

use crate::config::StrategyType;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Trait for load balancing selection.
pub trait SelectionStrategy: Send + Sync {
    /// Select an index from 0..len.
    fn select_index(&self, len: usize) -> usize;
}

/// Round-robin selection strategy.
///
/// Thread-safe using atomic counter.
pub struct RoundRobinStrategy {
    counter: AtomicUsize,
}

impl RoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionStrategy for RoundRobinStrategy {
    fn select_index(&self, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        let index = self.counter.fetch_add(1, Ordering::Relaxed);
        index % len
    }
}

/// Random selection strategy.
///
/// Uses a simple hash-based approach for randomness without external deps.
pub struct RandomStrategy;

impl RandomStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RandomStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionStrategy for RandomStrategy {
    fn select_index(&self, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize)
            .unwrap_or(0);
        // Mix with a static random seed for better distribution
        let mixed = nanos.wrapping_mul(0x517cc1b727220a95);
        mixed % len
    }
}

/// Create a selection strategy from config.
pub fn create_strategy(strategy_type: StrategyType) -> Box<dyn SelectionStrategy> {
    match strategy_type {
        StrategyType::RoundRobin => Box::new(RoundRobinStrategy::new()),
        StrategyType::Random => Box::new(RandomStrategy::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin_distribution() {
        let strategy = RoundRobinStrategy::new();
        let mut counts = vec![0usize; 3];
        for _ in 0..30 {
            let idx = strategy.select_index(3);
            counts[idx] += 1;
        }
        // Each index should be selected exactly 10 times
        assert_eq!(counts, vec![10, 10, 10]);
    }

    #[test]
    fn test_round_robin_empty() {
        let strategy = RoundRobinStrategy::new();
        assert_eq!(strategy.select_index(0), 0);
    }

    #[test]
    fn test_random_valid_range() {
        let strategy = RandomStrategy::new();
        for _ in 0..100 {
            let idx = strategy.select_index(5);
            assert!(idx < 5);
        }
    }

    #[test]
    fn test_random_empty() {
        let strategy = RandomStrategy::new();
        assert_eq!(strategy.select_index(0), 0);
    }

    #[test]
    fn test_create_strategy() {
        let s = create_strategy(StrategyType::RoundRobin);
        assert_eq!(s.select_index(1), 0);

        let s = create_strategy(StrategyType::Random);
        assert_eq!(s.select_index(1), 0);
    }
}