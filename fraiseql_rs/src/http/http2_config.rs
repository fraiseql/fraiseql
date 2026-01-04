//! HTTP/2 Protocol Configuration and Optimization (Phase 18)
//!
//! This module provides configuration and utilities for HTTP/2 multiplexing,
//! connection pooling, and stream management to achieve 20-50% throughput improvement.

use std::num::NonZeroUsize;

/// HTTP/2 Configuration for Hyper server
///
/// Controls HTTP/2 specific settings including:
/// - Stream limits and flow control windows
/// - Frame sizes and buffer tuning
/// - Connection pooling parameters
#[derive(Debug, Clone)]
pub struct Http2Config {
    /// Maximum number of concurrent streams per connection
    /// Default: 500 (balanced throughput vs memory)
    /// High Performance: 1000
    /// Conservative: 100
    pub max_concurrent_streams: NonZeroUsize,

    /// Initial flow control window size in bytes (per stream)
    /// Default: 128KB (64*1024)
    /// High Throughput: 256KB (256*1024)
    /// Low Latency: 64KB (64*1024)
    pub initial_window_size: u32,

    /// Connection-level flow control window in bytes
    /// Default: 1MB (1024*1024)
    /// High Throughput: 10MB (10*1024*1024)
    pub connection_window_size: u32,

    /// Maximum HTTP/2 frame size in bytes (standard: 16KB)
    /// Frames are negotiated via SETTINGS frame, kept at standard
    pub max_frame_size: u32,

    /// Header table size for HPACK compression in bytes
    /// Default: 4096 (standard)
    pub header_table_size: u32,

    /// Enable server push (experimental, usually false)
    pub enable_push: bool,

    /// TCP nodelay: disable Nagle's algorithm for lower latency
    pub tcp_nodelay: bool,

    /// TCP keep-alive: keep idle connections alive
    pub tcp_keepalive: bool,

    /// SO_RCVBUF: Receive buffer size in bytes
    /// Default: 512KB
    pub recv_buffer_size: usize,

    /// SO_SNDBUF: Send buffer size in bytes
    /// Default: 512KB
    pub send_buffer_size: usize,
}

impl Http2Config {
    /// Balanced configuration (recommended for most SaaS)
    /// - 500 streams per connection
    /// - 128KB initial window
    /// - 1MB connection window
    /// - 512KB socket buffers
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            max_concurrent_streams: unsafe { NonZeroUsize::new_unchecked(500) },
            initial_window_size: 128 * 1024,     // 128KB
            connection_window_size: 1024 * 1024, // 1MB
            max_frame_size: 16 * 1024,           // 16KB (standard)
            header_table_size: 4096,
            enable_push: false,
            tcp_nodelay: true,
            tcp_keepalive: true,
            recv_buffer_size: 512 * 1024,        // 512KB
            send_buffer_size: 512 * 1024,        // 512KB
        }
    }

    /// High throughput configuration (for maximum request rate)
    /// - 1000 streams per connection
    /// - 256KB initial window
    /// - 10MB connection window
    /// - 1MB socket buffers
    #[must_use]
    pub const fn high_throughput() -> Self {
        Self {
            max_concurrent_streams: unsafe { NonZeroUsize::new_unchecked(1000) },
            initial_window_size: 256 * 1024,            // 256KB
            connection_window_size: 10 * 1024 * 1024,   // 10MB
            max_frame_size: 16 * 1024,                  // 16KB
            header_table_size: 4096,
            enable_push: false,
            tcp_nodelay: true,
            tcp_keepalive: true,
            recv_buffer_size: 1024 * 1024,              // 1MB
            send_buffer_size: 1024 * 1024,              // 1MB
        }
    }

    /// Low latency configuration (for responsive applications)
    /// - 100 streams per connection
    /// - 64KB initial window (less buffering)
    /// - 256KB connection window
    /// - 256KB socket buffers
    #[must_use]
    pub const fn low_latency() -> Self {
        Self {
            max_concurrent_streams: unsafe { NonZeroUsize::new_unchecked(100) },
            initial_window_size: 64 * 1024,      // 64KB
            connection_window_size: 256 * 1024,  // 256KB
            max_frame_size: 16 * 1024,           // 16KB
            header_table_size: 4096,
            enable_push: false,
            tcp_nodelay: true,
            tcp_keepalive: true,
            recv_buffer_size: 256 * 1024,        // 256KB
            send_buffer_size: 256 * 1024,        // 256KB
        }
    }

    /// Conservative configuration (for stability/backwards compatibility)
    /// - 50 streams per connection
    /// - 64KB initial window
    /// - 128KB connection window
    /// - Smaller buffers for memory efficiency
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            max_concurrent_streams: unsafe { NonZeroUsize::new_unchecked(50) },
            initial_window_size: 64 * 1024,       // 64KB
            connection_window_size: 128 * 1024,   // 128KB
            max_frame_size: 16 * 1024,            // 16KB
            header_table_size: 4096,
            enable_push: false,
            tcp_nodelay: true,
            tcp_keepalive: true,
            recv_buffer_size: 256 * 1024,         // 256KB
            send_buffer_size: 256 * 1024,         // 256KB
        }
    }
}

impl Default for Http2Config {
    fn default() -> Self {
        Self::balanced()
    }
}

/// HTTP/2 Multiplexing Statistics (for observability)
#[derive(Debug, Clone)]
pub struct Http2Stats {
    /// Total number of HTTP/2 streams opened
    pub streams_opened_total: u64,

    /// Total number of streams closed
    pub streams_closed_total: u64,

    /// Currently active streams across all connections
    pub streams_active_current: u64,

    /// Average streams per connection (multiplexing efficiency)
    pub streams_per_connection_avg: f64,

    /// Peak active streams seen
    pub streams_peak: u64,

    /// Total connections using HTTP/2
    pub h2_connections_total: u64,

    /// Number of HTTP/1.1 connections (for comparison)
    pub h1_connections_total: u64,

    /// Total flow control windows exceeded (backpressure events)
    pub flow_control_events: u64,
}

impl Default for Http2Stats {
    fn default() -> Self {
        Self {
            streams_opened_total: 0,
            streams_closed_total: 0,
            streams_active_current: 0,
            streams_per_connection_avg: 0.0,
            streams_peak: 0,
            h2_connections_total: 0,
            h1_connections_total: 0,
            flow_control_events: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http2_config_balanced() {
        let config = Http2Config::balanced();
        assert_eq!(config.max_concurrent_streams.get(), 500);
        assert_eq!(config.initial_window_size, 128 * 1024);
        assert_eq!(config.connection_window_size, 1024 * 1024);
    }

    #[test]
    fn test_http2_config_high_throughput() {
        let config = Http2Config::high_throughput();
        assert_eq!(config.max_concurrent_streams.get(), 1000);
        assert_eq!(config.initial_window_size, 256 * 1024);
        assert_eq!(config.connection_window_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_http2_config_low_latency() {
        let config = Http2Config::low_latency();
        assert_eq!(config.max_concurrent_streams.get(), 100);
        assert_eq!(config.initial_window_size, 64 * 1024);
    }

    #[test]
    fn test_http2_config_default() {
        let config = Http2Config::default();
        // Should match balanced
        assert_eq!(config.max_concurrent_streams.get(), 500);
    }

    #[test]
    fn test_http2_config_tcp_options() {
        let config = Http2Config::balanced();
        assert!(config.tcp_nodelay);
        assert!(config.tcp_keepalive);
    }

    #[test]
    fn test_http2_stats_default() {
        let stats = Http2Stats::default();
        assert_eq!(stats.streams_opened_total, 0);
        assert_eq!(stats.streams_active_current, 0);
    }

    #[test]
    fn test_http2_buffer_sizes() {
        let balanced = Http2Config::balanced();
        let high_throughput = Http2Config::high_throughput();
        let low_latency = Http2Config::low_latency();

        // Verify buffer sizes scale with performance target
        assert!(balanced.recv_buffer_size < high_throughput.recv_buffer_size);
        assert!(low_latency.recv_buffer_size < balanced.recv_buffer_size);
    }
}
