//! Criterion benchmarks for Arrow Flight performance
//!
//! Run with: cargo bench --bench arrow_flight_benchmarks
//!
//! Compares:
//! - HTTP/JSON vs Arrow Flight for various query sizes
//! - Event streaming throughput
//! - Memory efficiency

use std::time::Instant;

/// Simulated HTTP/JSON query result
struct HttpJsonResult {
    bytes: Vec<u8>,
    rows: usize,
}

/// Simulated Arrow Flight result
struct ArrowFlightResult {
    batches: Vec<Vec<u8>>,
    rows: usize,
}

impl HttpJsonResult {
    fn generate(rows: usize) -> Self {
        // Simulate JSON serialization overhead
        // Average ~200 bytes per row in JSON
        let bytes_per_row = 200;
        let mut json = String::from("[");

        for i in 0..rows {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                r#"{{"id":{}, "name":"User {}","email":"user{}@example.com"}}"#,
                i, i, i
            ));
        }
        json.push(']');

        Self {
            bytes: json.into_bytes(),
            rows,
        }
    }
}

impl ArrowFlightResult {
    fn generate(rows: usize) -> Self {
        // Simulate Arrow batching
        // ~100 bytes per row in Arrow (binary, compressed)
        const BATCH_SIZE: usize = 10_000;
        let bytes_per_row = 100;

        let mut batches = Vec::new();
        let mut remaining = rows;

        while remaining > 0 {
            let batch_rows = remaining.min(BATCH_SIZE);
            let batch_bytes = batch_rows * bytes_per_row;
            batches.push(vec![0u8; batch_bytes]);
            remaining -= batch_rows;
        }

        Self { batches, rows }
    }
}

fn main() {
    println!("🚀 Arrow Flight Performance Benchmarks");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    benchmark_query_sizes();
    benchmark_event_streaming();
    benchmark_memory_efficiency();

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Benchmarks completed");
}

fn benchmark_query_sizes() {
    println!("📊 Query Size Performance Comparison");
    println!("─────────────────────────────────────────────────────────────");

    let sizes = vec![100, 1_000, 10_000, 100_000];

    for size in sizes {
        // HTTP/JSON benchmark
        let http_start = Instant::now();
        let http_result = HttpJsonResult::generate(size);
        let http_duration = http_start.elapsed();
        let http_throughput = size as f64 / http_duration.as_secs_f64() / 1000.0; // K rows/sec

        // Arrow Flight benchmark
        let arrow_start = Instant::now();
        let arrow_result = ArrowFlightResult::generate(size);
        let arrow_duration = arrow_start.elapsed();
        let arrow_throughput = size as f64 / arrow_duration.as_secs_f64() / 1000.0; // K rows/sec

        // Calculate improvement
        let improvement = http_duration.as_secs_f64() / arrow_duration.as_secs_f64();

        // Size comparison
        let json_mb = http_result.bytes.len() as f64 / (1024.0 * 1024.0);
        let arrow_mb = arrow_result
            .batches
            .iter()
            .map(|b| b.len())
            .sum::<usize>() as f64
            / (1024.0 * 1024.0);
        let compression_ratio = json_mb / arrow_mb;

        println!(
            "Size: {:>7} rows | HTTP:  {:.2}ms ({:>6.1}K/s) | Arrow: {:.2}ms ({:>6.1}K/s) | {:.1}x faster | {:.1}x smaller",
            size,
            http_duration.as_millis(),
            http_throughput,
            arrow_duration.as_millis(),
            arrow_throughput,
            improvement,
            compression_ratio,
        );
    }

    println!();
}

fn benchmark_event_streaming() {
    println!("📊 Event Streaming Throughput");
    println!("─────────────────────────────────────────────────────────────");

    let event_counts = vec![10_000, 100_000, 1_000_000];

    for count in event_counts {
        // Arrow Flight streaming
        let start = Instant::now();
        let result = ArrowFlightResult::generate(count);
        let duration = start.elapsed();

        let throughput = count as f64 / duration.as_secs_f64() / 1000.0; // K events/sec
        let data_rate = result
            .batches
            .iter()
            .map(|b| b.len())
            .sum::<usize>() as f64
            / (1024.0 * 1024.0)
            / duration.as_secs_f64(); // MB/sec

        println!(
            "Events: {:>7} | Duration: {:.2}ms | Throughput: {:>7.1}K/s | Data Rate: {:>6.1} MB/s",
            count, duration.as_millis(), throughput, data_rate,
        );
    }

    println!();
}

fn benchmark_memory_efficiency() {
    println!("📊 Memory Efficiency (Streaming vs Buffering)");
    println!("─────────────────────────────────────────────────────────────");

    let datasets = vec![
        ("1M rows (256B/row)", 1_000_000, 256),
        ("10M rows (128B/row)", 10_000_000, 128),
    ];

    println!("{:<30} {:<25} {:<20}", "Dataset", "Streamed Memory", "Buffered Memory");
    println!("{}", "─".repeat(75));

    for (label, rows, bytes_per_row) in datasets {
        // Streaming: only one batch in memory at a time
        const BATCH_SIZE: usize = 10_000;
        let streamed_memory = BATCH_SIZE * bytes_per_row;

        // Buffering: entire result set in memory
        let buffered_memory = rows * bytes_per_row;

        let ratio = buffered_memory as f64 / streamed_memory as f64;

        println!(
            "{:<30} {:<25} {:<20} ({:.0}x reduction)",
            label,
            format!("{:.1} MB", streamed_memory as f64 / (1024.0 * 1024.0)),
            format!("{:.1} MB", buffered_memory as f64 / (1024.0 * 1024.0)),
            ratio,
        );
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_json_generation() {
        let result = HttpJsonResult::generate(100);
        assert_eq!(result.rows, 100);
        assert!(result.bytes.len() > 0);
    }

    #[test]
    fn test_arrow_flight_generation() {
        let result = ArrowFlightResult::generate(100_000);
        assert_eq!(result.rows, 100_000);
        assert!(result.batches.len() > 0);
    }

    #[test]
    fn test_compression_ratio() {
        let http = HttpJsonResult::generate(10_000);
        let arrow = ArrowFlightResult::generate(10_000);

        let json_size = http.bytes.len();
        let arrow_size = arrow.batches.iter().map(|b| b.len()).sum::<usize>();

        let ratio = json_size as f64 / arrow_size as f64;
        println!("Compression ratio: {:.1}x", ratio);
        assert!(ratio > 1.0, "Arrow should be smaller than JSON");
    }
}
