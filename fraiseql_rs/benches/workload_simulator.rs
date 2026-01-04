//! Realistic workload simulator for cache performance validation
//!
//! Provides configurable query workload generators that simulate real-world
//! usage patterns: typical SaaS, high-frequency APIs, and analytical queries.

use rand::distributions::{Distribution, WeightedIndex, Zipfian};
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A generated query with parameters
#[derive(Debug, Clone)]
pub struct GeneratedQuery {
    /// The base query template (e.g., "query { user(id: %d) { id name email } }")
    pub template: String,

    /// Unique identifier for deduplication
    pub query_id: String,

    /// Whether this query is expected to hit cache (based on distribution)
    pub should_hit_cache: bool,

    /// Approximate response size in bytes
    pub response_size: usize,
}

/// Statistics about generated workload
#[derive(Debug, Clone)]
pub struct WorkloadStats {
    /// Total queries generated
    pub total_queries: u64,

    /// Number of unique query patterns
    pub unique_queries: u64,

    /// Expected cache hit rate (0.0 to 1.0)
    pub expected_hit_rate: f64,

    /// Average response size in bytes
    pub avg_response_size: usize,

    /// Queries per second (for reference)
    pub qps: f64,
}

/// Workload profile - describes usage patterns
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkloadProfile {
    /// Typical SaaS: 60% repeated hot queries, 40% unique cold queries
    /// Expected hit rate: 80-90%
    TypicalSaaS,

    /// High-frequency API: 90% repeated queries, 10% unique
    /// Expected hit rate: 90-95%
    HighFrequencyApi,

    /// Analytical: 30% repeated, 70% unique queries
    /// Expected hit rate: 20-40%
    Analytical,

    /// Custom: specify exact hit rate and distribution
    Custom { hit_rate: f64 },
}

impl WorkloadProfile {
    fn expected_hit_rate(&self) -> f64 {
        match self {
            Self::TypicalSaaS => 0.85,
            Self::HighFrequencyApi => 0.92,
            Self::Analytical => 0.30,
            Self::Custom { hit_rate } => *hit_rate,
        }
    }

    fn hot_query_percentage(&self) -> f64 {
        match self {
            Self::TypicalSaaS => 0.60,
            Self::HighFrequencyApi => 0.90,
            Self::Analytical => 0.30,
            Self::Custom { hit_rate } => hit_rate * 0.7,
        }
    }

    fn zipfian_skew(&self) -> f64 {
        // How skewed the distribution is toward popular queries
        match self {
            Self::TypicalSaaS => 1.5,       // Moderate skew (Pareto-ish)
            Self::HighFrequencyApi => 2.0,  // High skew (80/20 rule)
            Self::Analytical => 0.5,        // Low skew (more uniform)
            Self::Custom { .. } => 1.2,     // Default
        }
    }
}

/// Query generator for a specific workload profile
pub struct WorkloadGenerator {
    /// Profile this generator follows
    profile: WorkloadProfile,

    /// Available user IDs
    user_ids: Vec<u32>,

    /// Available entity IDs
    entity_ids: Vec<u32>,

    /// Available date ranges
    date_ranges: Vec<String>,

    /// Pre-generated hot queries (frequently accessed)
    hot_queries: Vec<GeneratedQuery>,

    /// Pre-generated cold queries (rarely accessed)
    cold_queries: Vec<GeneratedQuery>,

    /// RNG for query selection
    rng: rand::rngs::StdRng,

    /// Distribution for hot query selection (Zipfian)
    hot_dist: WeightedIndex<f64>,

    /// Stats tracking
    stats: Arc<WorkloadStats>,

    /// Query counter
    query_count: Arc<AtomicU64>,
}

impl WorkloadGenerator {
    /// Create a new workload generator for the given profile
    pub fn new(profile: WorkloadProfile) -> Self {
        use rand::SeedableRng;

        // Setup user and entity IDs
        let user_ids: Vec<u32> = (1..=1000).collect();
        let entity_ids: Vec<u32> = (1..=10000).collect();
        let date_ranges = vec![
            "2024-01-01".to_string(),
            "2024-02-01".to_string(),
            "2024-03-01".to_string(),
            "2024-04-01".to_string(),
            "2024-05-01".to_string(),
        ];

        let rng = rand::rngs::StdRng::from_entropy();

        // Generate hot queries
        let hot_count = (100.0 * profile.hot_query_percentage()) as usize;
        let hot_queries = (0..hot_count)
            .map(|i| Self::generate_query(i as u32, true, &user_ids, &entity_ids, &date_ranges))
            .collect();

        // Generate cold queries
        let cold_count = 100 - hot_count;
        let cold_queries = (0..cold_count)
            .map(|i| {
                Self::generate_query((hot_count as u32 + i as u32), false, &user_ids, &entity_ids, &date_ranges)
            })
            .collect();

        // Create weighted distribution for hot queries (Zipfian)
        let weights: Vec<f64> = (1..=hot_count)
            .map(|i| 1.0 / (i as f64).powf(profile.zipfian_skew()))
            .collect();
        let hot_dist = WeightedIndex::new(&weights).expect("valid weights");

        let expected_hit_rate = profile.expected_hit_rate();
        let stats = Arc::new(WorkloadStats {
            total_queries: 0,
            unique_queries: hot_count as u64 + cold_count as u64,
            expected_hit_rate,
            avg_response_size: 5000, // Average 5KB response
            qps: 0.0,
        });

        Self {
            profile,
            user_ids,
            entity_ids,
            date_ranges,
            hot_queries,
            cold_queries,
            rng,
            hot_dist,
            stats,
            query_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Generate the next query in the workload
    pub fn next_query(&mut self) -> GeneratedQuery {
        let hit_rate = self.profile.expected_hit_rate();
        let should_hit_cache = self.rng.gen::<f64>() < hit_rate;

        let query = if should_hit_cache {
            // Select from hot queries using Zipfian distribution
            let idx = self.hot_dist.sample(&mut self.rng);
            self.hot_queries[idx].clone()
        } else {
            // Select randomly from cold queries
            let idx = self.rng.gen_range(0..self.cold_queries.len());
            self.cold_queries[idx].clone()
        };

        self.query_count.fetch_add(1, Ordering::Relaxed);
        query
    }

    /// Generate N queries
    pub fn next_batch(&mut self, count: usize) -> Vec<GeneratedQuery> {
        (0..count).map(|_| self.next_query()).collect()
    }

    /// Get current statistics
    pub fn stats(&self) -> WorkloadStats {
        let count = self.query_count.load(Ordering::Relaxed);
        WorkloadStats {
            total_queries: count,
            unique_queries: self.stats.unique_queries,
            expected_hit_rate: self.stats.expected_hit_rate,
            avg_response_size: self.stats.avg_response_size,
            qps: 0.0, // Updated by benchmark harness
        }
    }

    /// Reset the generator
    pub fn reset(&mut self) {
        self.query_count.store(0, Ordering::Relaxed);
    }

    /// Helper: generate a single query
    fn generate_query(
        seed: u32,
        _is_hot: bool,
        user_ids: &[u32],
        entity_ids: &[u32],
        date_ranges: &[String],
    ) -> GeneratedQuery {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed as u64);

        let query_type = match rng.gen_range(0..3) {
            0 => "user",
            1 => "post",
            _ => "comment",
        };

        // Generate realistic parameters
        let id = entity_ids[rng.gen_range(0..entity_ids.len())];
        let user_id = user_ids[rng.gen_range(0..user_ids.len())];
        let date = &date_ranges[rng.gen_range(0..date_ranges.len())];

        let template = match query_type {
            "user" => format!(
                r#"query {{ user(id: {}) {{ id name email createdAt }} }}"#,
                id
            ),
            "post" => format!(
                r#"query {{ post(id: {}) {{ id title body author {{ id name }} createdAt }} }}"#,
                id
            ),
            _ => format!(
                r#"query {{ userPosts(userId: {}, date: "{}") {{ id title author {{ id }} }} }}"#,
                user_id, date
            ),
        };

        let query_id = format!("{}_{}", query_type, id);
        let response_size = rng.gen_range(1000..10000); // 1-10KB responses

        GeneratedQuery {
            template,
            query_id,
            should_hit_cache: false, // Updated by next_query()
            response_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typical_saas_profile() {
        let profile = WorkloadProfile::TypicalSaaS;
        assert_eq!(profile.expected_hit_rate(), 0.85);
        assert!(profile.hot_query_percentage() > 0.5);
    }

    #[test]
    fn test_high_frequency_api_profile() {
        let profile = WorkloadProfile::HighFrequencyApi;
        assert!(profile.expected_hit_rate() > 0.90);
        assert!(profile.hot_query_percentage() > 0.80);
    }

    #[test]
    fn test_analytical_profile() {
        let profile = WorkloadProfile::Analytical;
        assert!(profile.expected_hit_rate() < 0.50);
        assert!(profile.hot_query_percentage() < 0.50);
    }

    #[test]
    fn test_workload_generator() {
        let mut gen = WorkloadGenerator::new(WorkloadProfile::TypicalSaaS);

        // Generate 100 queries
        let queries = gen.next_batch(100);
        assert_eq!(queries.len(), 100);

        // Verify queries have expected fields
        for query in &queries {
            assert!(!query.query_id.is_empty());
            assert!(!query.template.is_empty());
            assert!(query.response_size > 0);
        }

        // Check stats
        let stats = gen.stats();
        assert_eq!(stats.total_queries, 100);
        assert!(stats.expected_hit_rate > 0.8);
    }

    #[test]
    fn test_hit_rate_distribution() {
        let mut gen = WorkloadGenerator::new(WorkloadProfile::TypicalSaaS);
        let expected_hit_rate = 0.85;

        // Generate many queries and check hit rate distribution
        let queries: Vec<_> = (0..1000).map(|_| gen.next_query()).collect();

        let hits = queries.iter().filter(|q| q.should_hit_cache).count();
        let actual_hit_rate = hits as f64 / queries.len() as f64;

        // Allow ±10% deviation from expected
        assert!((actual_hit_rate - expected_hit_rate).abs() < 0.10,
            "Hit rate {:.2}% vs expected {:.2}%",
            actual_hit_rate * 100.0,
            expected_hit_rate * 100.0
        );
    }

    #[test]
    fn test_zipfian_distribution() {
        let mut gen = WorkloadGenerator::new(WorkloadProfile::HighFrequencyApi);
        let mut query_frequency = HashMap::new();

        // Generate many queries
        for _ in 0..5000 {
            let query = gen.next_query();
            if query.should_hit_cache {
                *query_frequency.entry(query.query_id).or_insert(0) += 1;
            }
        }

        // Find top queries
        let mut freq_vec: Vec<_> = query_frequency.into_iter().collect();
        freq_vec.sort_by(|a, b| b.1.cmp(&a.1));

        // Top 20% queries should account for 80%+ of traffic (Zipfian)
        let total: usize = freq_vec.iter().map(|(_, f)| f).sum();
        let top_20_percent = freq_vec.len() / 5;
        let top_20_count: usize = freq_vec[..top_20_percent].iter().map(|(_, f)| f).sum();
        let top_20_rate = top_20_count as f64 / total as f64;

        assert!(top_20_rate > 0.70, "Top 20% should be >70% of traffic, got {:.0}%", top_20_rate * 100.0);
    }
}
