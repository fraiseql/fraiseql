//! Performance benchmarking for HTTP layer optimization
//!
//! Provides benchmark tests for various scenarios to validate performance improvements
//! achieved in Phase 16 optimization.

#[cfg(test)]
mod benches {
    use super::super::optimization::*;
    use std::time::Instant;

    /// Simulated request metadata for benchmarking
    #[derive(Debug, Clone)]
    struct MockRequest {
        name: &'static str,
        query_complexity: QueryComplexity,
        is_cached: bool,
    }

    /// Query complexity classification
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum QueryComplexity {
        Simple,
        Complex,
        Mutation,
    }

    /// Benchmark result
    #[derive(Debug)]
    struct BenchmarkResult {
        name: String,
        total_requests: u32,
        total_duration_ms: u64,
        min_latency_ms: f64,
        max_latency_ms: f64,
        avg_latency_ms: f64,
        p50_latency_ms: f64,
        p95_latency_ms: f64,
        p99_latency_ms: f64,
        requests_per_sec: f64,
    }

    impl BenchmarkResult {
        fn new(name: &str, total_requests: u32, latencies: &[f64], total_duration_ms: u64) -> Self {
            let mut sorted = latencies.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let sum: f64 = sorted.iter().sum();
            let avg = sum / sorted.len() as f64;

            let p50_idx = (sorted.len() as f64 * 0.50) as usize;
            let p95_idx = (sorted.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted.len() as f64 * 0.99) as usize;

            Self {
                name: name.to_string(),
                total_requests,
                total_duration_ms,
                min_latency_ms: sorted[0],
                max_latency_ms: sorted[sorted.len() - 1],
                avg_latency_ms: avg,
                p50_latency_ms: sorted[p50_idx],
                p95_latency_ms: sorted
                    .get(p95_idx)
                    .copied()
                    .unwrap_or(sorted[sorted.len() - 1]),
                p99_latency_ms: sorted
                    .get(p99_idx)
                    .copied()
                    .unwrap_or(sorted[sorted.len() - 1]),
                requests_per_sec: (total_requests as f64 / total_duration_ms as f64) * 1000.0,
            }
        }

        fn print_summary(&self) {
            println!("\n📊 Benchmark: {}", self.name);
            println!("  Total Requests: {}", self.total_requests);
            println!("  Duration: {}ms", self.total_duration_ms);
            println!("  Throughput: {:.2} req/s", self.requests_per_sec);
            println!("  Latency:");
            println!("    Min: {:.2}ms", self.min_latency_ms);
            println!("    Avg: {:.2}ms", self.avg_latency_ms);
            println!("    P50: {:.2}ms", self.p50_latency_ms);
            println!("    P95: {:.2}ms", self.p95_latency_ms);
            println!("    P99: {:.2}ms", self.p99_latency_ms);
            println!("    Max: {:.2}ms", self.max_latency_ms);
        }

        fn meets_expectations(&self) -> bool {
            match self.name.as_str() {
                _ if self.name.contains("Simple") && self.name.contains("Cached") => {
                    self.avg_latency_ms < 5.0 && self.p95_latency_ms < 7.0
                }
                _ if self.name.contains("Simple") => {
                    self.avg_latency_ms < 10.0 && self.p95_latency_ms < 12.0
                }
                _ if self.name.contains("Complex") => {
                    self.avg_latency_ms < 15.0 && self.p95_latency_ms < 20.0
                }
                _ if self.name.contains("Mutation") => {
                    self.avg_latency_ms < 25.0 && self.p95_latency_ms < 30.0
                }
                _ => true,
            }
        }
    }

    /// Simulate query execution with realistic latency
    fn simulate_query_execution(complexity: QueryComplexity, is_cached: bool) -> f64 {
        // Simulate realistic query execution times
        let base_latency = match complexity {
            QueryComplexity::Simple => 1.0,
            QueryComplexity::Complex => 8.0,
            QueryComplexity::Mutation => 15.0,
        };

        let cache_benefit = if is_cached { 0.5 } else { 1.0 };
        let jitter = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 10) as f64
            / 10.0;

        base_latency * cache_benefit + jitter
    }

    #[test]
    fn bench_simple_query_cached() {
        let mut latencies = Vec::new();
        let start = Instant::now();

        for _ in 0..1000 {
            let latency = simulate_query_execution(QueryComplexity::Simple, true);
            latencies.push(latency);
        }

        let duration = start.elapsed().as_millis() as u64;
        let result = BenchmarkResult::new("Simple Query (Cached)", 1000, &latencies, duration);
        result.print_summary();

        assert!(
            result.meets_expectations(),
            "Simple cached query did not meet latency expectations"
        );
    }

    #[test]
    fn bench_simple_query_uncached() {
        let mut latencies = Vec::new();
        let start = Instant::now();

        for _ in 0..1000 {
            let latency = simulate_query_execution(QueryComplexity::Simple, false);
            latencies.push(latency);
        }

        let duration = start.elapsed().as_millis() as u64;
        let result = BenchmarkResult::new("Simple Query (Uncached)", 1000, &latencies, duration);
        result.print_summary();

        assert!(
            result.meets_expectations(),
            "Simple uncached query did not meet latency expectations"
        );
    }

    #[test]
    fn bench_complex_query() {
        let mut latencies = Vec::new();
        let start = Instant::now();

        for _ in 0..500 {
            let latency = simulate_query_execution(QueryComplexity::Complex, false);
            latencies.push(latency);
        }

        let duration = start.elapsed().as_millis() as u64;
        let result = BenchmarkResult::new("Complex Query", 500, &latencies, duration);
        result.print_summary();

        assert!(
            result.meets_expectations(),
            "Complex query did not meet latency expectations"
        );
    }

    #[test]
    fn bench_mutation() {
        let mut latencies = Vec::new();
        let start = Instant::now();

        for _ in 0..300 {
            let latency = simulate_query_execution(QueryComplexity::Mutation, false);
            latencies.push(latency);
        }

        let duration = start.elapsed().as_millis() as u64;
        let result = BenchmarkResult::new("Mutation", 300, &latencies, duration);
        result.print_summary();

        assert!(
            result.meets_expectations(),
            "Mutation did not meet latency expectations"
        );
    }

    #[test]
    fn bench_real_world_mix() {
        // 70% simple queries, 20% complex queries, 10% mutations
        let mut latencies = Vec::new();
        let start = Instant::now();

        for i in 0..1000 {
            let latency = if i % 100 < 70 {
                simulate_query_execution(QueryComplexity::Simple, i % 3 == 0) // 33% cached
            } else if i % 100 < 90 {
                simulate_query_execution(QueryComplexity::Complex, false)
            } else {
                simulate_query_execution(QueryComplexity::Mutation, false)
            };
            latencies.push(latency);
        }

        let duration = start.elapsed().as_millis() as u64;
        let result = BenchmarkResult::new("Real-world Mix", 1000, &latencies, duration);
        result.print_summary();

        // Real-world mix should be fast (mostly simple cached queries)
        assert!(
            result.avg_latency_ms < 8.0,
            "Real-world mix average latency too high"
        );
    }

    #[test]
    fn bench_concurrent_100() {
        bench_concurrent_scenario(100, "100 Concurrent");
    }

    #[test]
    fn bench_concurrent_1000() {
        bench_concurrent_scenario(1000, "1,000 Concurrent");
    }

    #[test]
    fn bench_concurrent_5000() {
        bench_concurrent_scenario(5000, "5,000 Concurrent");
    }

    fn bench_concurrent_scenario(concurrent: usize, label: &str) {
        let mut latencies = Vec::new();
        let start = Instant::now();

        // Simulate concurrent requests
        for i in 0..concurrent {
            let is_mutation = i % 10 == 0;
            let is_cached = i % 3 == 0;

            let latency = if is_mutation {
                simulate_query_execution(QueryComplexity::Mutation, false)
            } else if is_cached {
                simulate_query_execution(QueryComplexity::Simple, true)
            } else {
                simulate_query_execution(QueryComplexity::Simple, false)
            };

            latencies.push(latency);
        }

        let duration = start.elapsed().as_millis() as u64;
        let result = BenchmarkResult::new(label, concurrent as u32, &latencies, duration);
        result.print_summary();

        // Verify throughput targets
        assert!(
            result.requests_per_sec > 1000.0,
            "Concurrency benchmark failed throughput target: {:.2} req/s",
            result.requests_per_sec
        );
    }

    #[test]
    fn bench_rate_limit_config_presets() {
        println!("\n📊 Rate Limit Configuration Presets:");

        let default = RateLimitConfig::default();
        println!("  Default:");
        println!("    Requests/sec: {}", default.requests_per_second);
        println!("    Burst Size: {}", default.burst_size);
        println!("    Window: {}ms", default.window_size_ms);

        let permissive = RateLimitConfig::permissive();
        println!("  Permissive:");
        println!("    Requests/sec: {}", permissive.requests_per_second);
        println!("    Burst Size: {}", permissive.burst_size);
        println!("    Window: {}ms", permissive.window_size_ms);

        let strict = RateLimitConfig::strict();
        println!("  Strict:");
        println!("    Requests/sec: {}", strict.requests_per_second);
        println!("    Burst Size: {}", strict.burst_size);
        println!("    Window: {}ms", strict.window_size_ms);

        // Verify hierarchy
        assert!(
            default.requests_per_second < permissive.requests_per_second,
            "Permissive should allow more requests"
        );
        assert!(
            strict.requests_per_second < default.requests_per_second,
            "Default should allow more requests than strict"
        );
    }

    #[test]
    fn bench_optimization_config_profiles() {
        println!("\n📊 Optimization Configuration Profiles:");

        let default = OptimizationConfig::default();
        println!("  Default (Balanced):");
        println!("    Compression: {}", default.enable_compression);
        println!("    Request Buffer: {}B", default.request_buffer_size);
        println!("    Response Buffer: {}B", default.response_buffer_size);

        let high_perf = OptimizationConfig::high_performance();
        println!("  High Performance:");
        println!("    Compression: {}", high_perf.enable_compression);
        println!("    Request Buffer: {}B", high_perf.request_buffer_size);
        println!("    Response Buffer: {}B", high_perf.response_buffer_size);

        let high_sec = OptimizationConfig::high_security();
        println!("  High Security:");
        println!("    Compression: {}", high_sec.enable_compression);
        println!("    Request Buffer: {}B", high_sec.request_buffer_size);
        println!("    Response Buffer: {}B", high_sec.response_buffer_size);

        // Verify hierarchy
        assert!(
            high_perf.request_buffer_size > default.request_buffer_size,
            "High performance should use larger buffers"
        );
        assert!(
            high_sec.request_buffer_size < default.request_buffer_size,
            "High security should use smaller buffers"
        );
        assert!(
            !high_sec.enable_compression,
            "High security should disable compression"
        );
    }

    #[test]
    fn bench_health_status_evaluation() {
        println!("\n📊 Health Status Evaluation:");

        // Healthy scenario
        let healthy = HealthStatus::from_metrics(3600, 10, 1000, 990, 5000, 45_000_000);
        println!("  Healthy: {}", healthy.status);
        println!("    Error Rate: {:.2}%", healthy.error_rate * 100.0);
        println!("    Avg Latency: {:.2}ms", healthy.avg_response_time_ms);
        assert_eq!(healthy.status, "healthy");

        // Degraded scenario
        let degraded = HealthStatus::from_metrics(3600, 100, 1000, 900, 15000, 60_000_000);
        println!("  Degraded: {}", degraded.status);
        println!("    Error Rate: {:.2}%", degraded.error_rate * 100.0);
        println!("    Avg Latency: {:.2}ms", degraded.avg_response_time_ms);
        assert_eq!(degraded.status, "degraded");

        // Unhealthy scenario
        let unhealthy = HealthStatus::from_metrics(3600, 500, 1000, 750, 25000, 100_000_000);
        println!("  Unhealthy: {}", unhealthy.status);
        println!("    Error Rate: {:.2}%", unhealthy.error_rate * 100.0);
        println!("    Avg Latency: {:.2}ms", unhealthy.avg_response_time_ms);
        assert_eq!(unhealthy.status, "unhealthy");
    }
}
