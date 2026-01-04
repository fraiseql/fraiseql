//! HTTP/2 Buffer & Window Size Tuning (Phase 18.4)
//!
//! Fine-grained tuning of buffer sizes, flow control windows, and frame
//! sizes for optimal performance across different workload profiles.
//!
//! This module provides production configuration profiles and adaptive
//! tuning recommendations based on observed load patterns.

/// HTTP/2 Flow Control Window Configuration
///
/// Tuning flow control windows is critical for performance:
/// - Larger windows: Higher throughput, more buffering, higher memory
/// - Smaller windows: Lower latency, potential throughput loss
/// - Must balance against stream limits and connection count
#[derive(Debug, Clone)]
pub struct Http2FlowControlConfig {
    /// Initial stream-level flow control window (bytes)
    /// Default: 65KB (standard minimum)
    /// Range: 16KB-1MB
    /// Impact: Higher = less flow control stalls, more buffer memory
    pub initial_stream_window: u32,

    /// Connection-level flow control window (bytes)
    /// Default: 64KB (conservative)
    /// Range: 256KB-10MB
    /// Impact: Controls maximum concurrent throughput per connection
    pub initial_connection_window: u32,

    /// Maximum frame size (bytes)
    /// Standard: 16KB (16384), range: 16KB-16MB in HTTP/2 spec
    /// Recommendation: Keep at 16KB (standard), larger frames have diminishing returns
    pub max_frame_size: u32,

    /// Allow flow window to grow dynamically
    /// When true: Windows expand under sustained load
    /// When false: Fixed size (lower memory, potential throughput loss)
    pub allow_window_growth: bool,

    /// Maximum window size (if growth enabled)
    pub max_window_size: u32,

    /// Enable aggressive window consumption
    /// When true: Request larger windows to maximize throughput
    /// When false: Conservative window usage (less memory)
    pub aggressive_windowing: bool,
}

impl Http2FlowControlConfig {
    /// Balanced flow control (recommended for most workloads)
    /// - 64KB stream windows
    /// - 1MB connection window
    /// - Fixed windows (no growth)
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            initial_stream_window: 65536,        // 64KB (HTTP/2 default)
            initial_connection_window: 1048576,  // 1MB
            max_frame_size: 16384,               // 16KB (standard)
            allow_window_growth: false,
            max_window_size: 1048576,            // 1MB max
            aggressive_windowing: false,
        }
    }

    /// High throughput flow control
    /// - 256KB stream windows
    /// - 10MB connection window
    /// - Allow growth for sustained loads
    #[must_use]
    pub const fn high_throughput() -> Self {
        Self {
            initial_stream_window: 262144,       // 256KB
            initial_connection_window: 10485760, // 10MB
            max_frame_size: 16384,
            allow_window_growth: true,
            max_window_size: 10485760,           // 10MB max
            aggressive_windowing: true,
        }
    }

    /// Low latency flow control
    /// - 32KB stream windows (less buffering)
    /// - 256KB connection window (tight control)
    /// - Fixed windows for predictability
    #[must_use]
    pub const fn low_latency() -> Self {
        Self {
            initial_stream_window: 32768,        // 32KB
            initial_connection_window: 262144,   // 256KB
            max_frame_size: 16384,
            allow_window_growth: false,
            max_window_size: 262144,
            aggressive_windowing: false,
        }
    }

    /// Conservative flow control (memory constrained)
    /// - 16KB stream windows (minimal buffering)
    /// - 128KB connection window
    /// - No growth
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            initial_stream_window: 16384,        // 16KB
            initial_connection_window: 131072,   // 128KB
            max_frame_size: 16384,
            allow_window_growth: false,
            max_window_size: 131072,
            aggressive_windowing: false,
        }
    }
}

impl Default for Http2FlowControlConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// HTTP/2 Buffer Management Configuration
///
/// Controls read/write buffers for request/response handling
#[derive(Debug, Clone)]
pub struct Http2BufferConfig {
    /// Read buffer size (bytes)
    /// Used for incoming HTTP/2 frames
    /// Default: 256KB
    /// Impact: Larger = more data buffered, better throughput, higher memory
    pub read_buffer_size: usize,

    /// Write buffer size (bytes)
    /// Used for outgoing HTTP/2 frames
    /// Default: 256KB
    /// Impact: Larger = fewer flushes, better batching
    pub write_buffer_size: usize,

    /// Maximum body buffer size before streaming (bytes)
    /// Below this: fully buffered
    /// Above this: streamed (for large responses)
    pub body_streaming_threshold: usize,

    /// Header buffer size (for HPACK decompression)
    /// Default: 16KB
    /// Limits maximum header list size
    pub header_buffer_size: usize,

    /// Enable read buffer pooling (reuse allocations)
    /// When true: Reuse buffer allocations across requests
    /// Reduces GC pressure, improves cache locality
    pub enable_buffer_pooling: bool,

    /// Buffer pool size (number of buffers to keep)
    /// Only relevant if enable_buffer_pooling is true
    pub buffer_pool_size: usize,

    /// Flush interval for write buffer (milliseconds)
    /// 0 = flush on every write (low latency, worse throughput)
    /// >0 = batch writes for up to this many ms
    pub flush_interval_ms: u32,
}

impl Http2BufferConfig {
    /// Balanced buffer configuration
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            read_buffer_size: 262144,            // 256KB
            write_buffer_size: 262144,           // 256KB
            body_streaming_threshold: 1048576,   // 1MB
            header_buffer_size: 16384,           // 16KB
            enable_buffer_pooling: true,
            buffer_pool_size: 100,
            flush_interval_ms: 10,
        }
    }

    /// High throughput buffer configuration
    #[must_use]
    pub const fn high_throughput() -> Self {
        Self {
            read_buffer_size: 1048576,           // 1MB
            write_buffer_size: 1048576,          // 1MB
            body_streaming_threshold: 10485760,  // 10MB
            header_buffer_size: 32768,           // 32KB
            enable_buffer_pooling: true,
            buffer_pool_size: 500,
            flush_interval_ms: 50,
        }
    }

    /// Low latency buffer configuration
    #[must_use]
    pub const fn low_latency() -> Self {
        Self {
            read_buffer_size: 131072,            // 128KB
            write_buffer_size: 131072,           // 128KB
            body_streaming_threshold: 262144,    // 256KB (stream sooner)
            header_buffer_size: 8192,            // 8KB
            enable_buffer_pooling: false,        // Allocate fresh for latency
            buffer_pool_size: 0,
            flush_interval_ms: 0,                // Flush immediately
        }
    }

    /// Conservative buffer configuration (memory constrained)
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            read_buffer_size: 65536,             // 64KB
            write_buffer_size: 65536,            // 64KB
            body_streaming_threshold: 262144,    // 256KB
            header_buffer_size: 8192,            // 8KB
            enable_buffer_pooling: true,
            buffer_pool_size: 20,
            flush_interval_ms: 100,
        }
    }
}

impl Default for Http2BufferConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Combined HTTP/2 Tuning Profile
///
/// Integrates flow control and buffer configuration into a single
/// production-ready profile for different scenarios
#[derive(Debug, Clone)]
pub struct Http2TuningProfile {
    /// Name of the profile
    pub name: String,

    /// Flow control configuration
    pub flow_control: Http2FlowControlConfig,

    /// Buffer configuration
    pub buffers: Http2BufferConfig,

    /// Description of use case
    pub description: String,
}

impl Http2TuningProfile {
    /// Balanced profile: recommended for most SaaS
    /// - Moderate throughput (50-100K req/sec)
    /// - Moderate latency (5-10ms p99)
    /// - Reasonable memory usage
    #[must_use]
    pub fn balanced() -> Self {
        Self {
            name: "balanced".to_string(),
            flow_control: Http2FlowControlConfig::balanced(),
            buffers: Http2BufferConfig::balanced(),
            description: "Balanced throughput and latency for typical SaaS workloads".to_string(),
        }
    }

    /// High throughput profile: for massive scale
    /// - High throughput (100K-500K req/sec)
    /// - Moderate latency (10-20ms p99)
    /// - Higher memory usage acceptable
    #[must_use]
    pub fn high_throughput() -> Self {
        Self {
            name: "high_throughput".to_string(),
            flow_control: Http2FlowControlConfig::high_throughput(),
            buffers: Http2BufferConfig::high_throughput(),
            description: "Maximum throughput for high-scale deployments".to_string(),
        }
    }

    /// Low latency profile: for interactive applications
    /// - Moderate throughput (20-50K req/sec)
    /// - Low latency (<5ms p99)
    /// - Minimal buffering
    #[must_use]
    pub fn low_latency() -> Self {
        Self {
            name: "low_latency".to_string(),
            flow_control: Http2FlowControlConfig::low_latency(),
            buffers: Http2BufferConfig::low_latency(),
            description: "Minimal latency for real-time applications".to_string(),
        }
    }

    /// Conservative profile: for memory-constrained environments
    /// - Moderate throughput (10-30K req/sec)
    /// - Moderate latency (10-15ms p99)
    /// - Minimal memory footprint
    #[must_use]
    pub fn conservative() -> Self {
        Self {
            name: "conservative".to_string(),
            flow_control: Http2FlowControlConfig::conservative(),
            buffers: Http2BufferConfig::conservative(),
            description: "Memory-efficient for resource-constrained deployments".to_string(),
        }
    }

    /// Custom profile builder
    #[must_use]
    pub fn custom(
        name: impl Into<String>,
        flow_control: Http2FlowControlConfig,
        buffers: Http2BufferConfig,
    ) -> Self {
        Self {
            name: name.into(),
            flow_control,
            buffers,
            description: "Custom tuning profile".to_string(),
        }
    }
}

impl Default for Http2TuningProfile {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Tuning Recommendations based on workload analysis
#[derive(Debug, Clone)]
pub struct TuningRecommendation {
    /// Recommended profile name
    pub profile_name: String,

    /// Recommended peak throughput (req/sec)
    pub expected_throughput: u32,

    /// Expected p99 latency (milliseconds)
    pub expected_latency_p99_ms: u32,

    /// Expected memory overhead (MB)
    pub expected_memory_mb: u32,

    /// Rationale for recommendation
    pub rationale: String,

    /// Caveats and limitations
    pub caveats: Vec<String>,
}

impl TuningRecommendation {
    /// Generate recommendation based on workload characteristics
    #[must_use]
    pub fn recommend(
        target_throughput: u32,
        target_latency_p99_ms: u32,
        memory_constrained: bool,
    ) -> Self {
        // Choose profile based on requirements
        let profile_name = match (target_throughput, target_latency_p99_ms, memory_constrained) {
            // High throughput requirement
            (throughput, _, false) if throughput > 100_000 => "high_throughput",
            // Low latency requirement
            (_, latency, false) if latency < 5 => "low_latency",
            // Memory constrained
            (_, _, true) => "conservative",
            // Default to balanced
            _ => "balanced",
        };

        let (expected_throughput, expected_latency, expected_memory, rationale, caveats) =
            match profile_name {
                "high_throughput" => (
                    200_000,
                    15,
                    500,
                    "Optimized for maximum throughput with large buffers and windows".to_string(),
                    vec![
                        "Higher memory usage (500MB+)".to_string(),
                        "Latency may be higher due to buffering".to_string(),
                    ],
                ),
                "low_latency" => (
                    30_000,
                    3,
                    100,
                    "Optimized for minimum latency with small buffers".to_string(),
                    vec![
                        "Throughput may be reduced".to_string(),
                        "Frequent buffer flushes".to_string(),
                    ],
                ),
                "conservative" => (
                    20_000,
                    12,
                    50,
                    "Optimized for memory efficiency".to_string(),
                    vec![
                        "Lower throughput ceiling".to_string(),
                        "Suitable for resource-constrained environments".to_string(),
                    ],
                ),
                _ => (
                    50_000,
                    8,
                    200,
                    "Balanced profile suitable for typical SaaS workloads".to_string(),
                    vec![
                        "Good default for most use cases".to_string(),
                        "Can be tuned further based on actual load".to_string(),
                    ],
                ),
            };

        Self {
            profile_name: profile_name.to_string(),
            expected_throughput,
            expected_latency_p99_ms: expected_latency,
            expected_memory_mb: expected_memory,
            rationale,
            caveats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_control_balanced() {
        let config = Http2FlowControlConfig::balanced();
        assert_eq!(config.initial_stream_window, 65536);
        assert_eq!(config.initial_connection_window, 1048576);
        assert!(!config.allow_window_growth);
    }

    #[test]
    fn test_flow_control_high_throughput() {
        let config = Http2FlowControlConfig::high_throughput();
        assert!(config.initial_stream_window > 65536);
        assert!(config.initial_connection_window > 1048576);
        assert!(config.allow_window_growth);
        assert!(config.aggressive_windowing);
    }

    #[test]
    fn test_buffer_config_balanced() {
        let config = Http2BufferConfig::balanced();
        assert_eq!(config.read_buffer_size, 262144);
        assert!(config.enable_buffer_pooling);
        assert!(config.flush_interval_ms > 0);
    }

    #[test]
    fn test_buffer_config_low_latency() {
        let config = Http2BufferConfig::low_latency();
        assert!(config.read_buffer_size < 262144);
        assert!(!config.enable_buffer_pooling);
        assert_eq!(config.flush_interval_ms, 0); // Flush immediately
    }

    #[test]
    fn test_tuning_profile_defaults() {
        let profile = Http2TuningProfile::balanced();
        assert_eq!(profile.name, "balanced");

        let high_tp = Http2TuningProfile::high_throughput();
        assert_eq!(high_tp.name, "high_throughput");

        let low_lat = Http2TuningProfile::low_latency();
        assert_eq!(low_lat.name, "low_latency");
    }

    #[test]
    fn test_tuning_profile_custom() {
        let custom = Http2TuningProfile::custom(
            "my_profile",
            Http2FlowControlConfig::balanced(),
            Http2BufferConfig::balanced(),
        );
        assert_eq!(custom.name, "my_profile");
        assert_eq!(custom.description, "Custom tuning profile");
    }

    #[test]
    fn test_tuning_recommendation_high_throughput() {
        let rec = TuningRecommendation::recommend(200_000, 20, false);
        assert_eq!(rec.profile_name, "high_throughput");
        assert!(rec.expected_throughput > 100_000);
    }

    #[test]
    fn test_tuning_recommendation_low_latency() {
        let rec = TuningRecommendation::recommend(50_000, 3, false);
        assert_eq!(rec.profile_name, "low_latency");
        assert!(rec.expected_latency_p99_ms < 5);
    }

    #[test]
    fn test_tuning_recommendation_memory_constrained() {
        let rec = TuningRecommendation::recommend(100_000, 10, true);
        assert_eq!(rec.profile_name, "conservative");
        assert!(rec.expected_memory_mb < 100);
    }

    #[test]
    fn test_flow_control_window_ranges() {
        let balanced = Http2FlowControlConfig::balanced();
        let high_tp = Http2FlowControlConfig::high_throughput();

        // Verify scaling relationships
        assert!(high_tp.initial_stream_window > balanced.initial_stream_window);
        assert!(high_tp.initial_connection_window > balanced.initial_connection_window);
    }

    #[test]
    fn test_buffer_sizes_scaling() {
        let balanced = Http2BufferConfig::balanced();
        let high_tp = Http2BufferConfig::high_throughput();
        let low_lat = Http2BufferConfig::low_latency();

        // Verify throughput > balanced > latency in buffer sizes
        assert!(high_tp.read_buffer_size > balanced.read_buffer_size);
        assert!(balanced.read_buffer_size > low_lat.read_buffer_size);
    }
}
