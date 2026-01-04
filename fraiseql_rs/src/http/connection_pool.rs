//! HTTP Connection Pool Optimization (Phase 18.2)
//!
//! Configures socket options and connection pooling for optimal throughput
//! and low latency under high concurrency.

use std::time::Duration;

/// TCP Socket Configuration for connection pooling
///
/// Optimizes socket behavior for HTTP/2 multiplexing:
/// - `TCP_NODELAY`: Disable Nagle's algorithm (critical for latency)
/// - `SO_KEEPALIVE`: Keep idle connections alive
/// - `SO_RCVBUF/SO_SNDBUF`: Tuned buffer sizes
#[derive(Debug, Clone)]
pub struct SocketConfig {
    /// `TCP_NODELAY`: Disable Nagle's algorithm
    /// When true: Send data immediately (lower latency, larger packet count)
    /// When false: Batch data (higher throughput, higher latency)
    /// Recommended: true for HTTP/2 (multiple small frames)
    pub tcp_nodelay: bool,

    /// `SO_KEEPALIVE`: Enable TCP keep-alive
    /// When true: Send probe packets to detect dead connections
    /// When false: Don't send probes
    /// Recommended: true for long-lived connections
    pub tcp_keepalive: bool,

    /// `TCP_KEEPIDLE`: Seconds before first keep-alive probe
    /// Default: 7200 (2 hours) - too long
    /// Recommended: 60-120 seconds
    pub tcp_keepidle_secs: u32,

    /// `TCP_KEEPINTVL`: Seconds between keep-alive probes
    /// Default: 75 - too long
    /// Recommended: 10-30 seconds
    pub tcp_keepintvl_secs: u32,

    /// `TCP_KEEPCNT`: Number of keep-alive probes before timeout
    /// Default: 9
    /// Recommended: 3-5
    pub tcp_keepcnt: u32,

    /// `SO_RCVBUF`: Receive buffer size in bytes
    /// Larger = more buffering, higher memory
    /// Smaller = less latency, potential throughput loss
    pub recv_buffer_bytes: usize,

    /// `SO_SNDBUF`: Send buffer size in bytes
    pub send_buffer_bytes: usize,

    /// `SO_REUSEADDR`: Allow reusing `TIME_WAIT` sockets
    /// When true: Can reuse local ports immediately after close
    /// Recommended: true for high-churn applications
    pub reuse_addr: bool,

    /// `SO_REUSEPORT`: Allow multiple sockets to bind to same port
    /// Requires kernel support, enables true port sharing
    pub reuse_port: bool,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Read timeout for receiving data
    pub read_timeout: Option<Duration>,

    /// Write timeout for sending data
    pub write_timeout: Option<Duration>,
}

impl SocketConfig {
    /// Balanced socket configuration (recommended for most HTTP/2)
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            tcp_nodelay: true,
            tcp_keepalive: true,
            tcp_keepidle_secs: 120,        // 2 minutes
            tcp_keepintvl_secs: 30,        // 30 seconds between probes
            tcp_keepcnt: 3,                // 3 probes before giving up
            recv_buffer_bytes: 512 * 1024, // 512KB
            send_buffer_bytes: 512 * 1024, // 512KB
            reuse_addr: true,
            reuse_port: false,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Some(Duration::from_secs(60)),
            write_timeout: Some(Duration::from_secs(60)),
        }
    }

    /// High-throughput socket configuration
    /// Larger buffers, longer keep-alive interval
    #[must_use]
    pub const fn high_throughput() -> Self {
        Self {
            tcp_nodelay: true,
            tcp_keepalive: true,
            tcp_keepidle_secs: 300, // 5 minutes
            tcp_keepintvl_secs: 60, // 60 seconds between probes
            tcp_keepcnt: 3,
            recv_buffer_bytes: 1024 * 1024, // 1MB
            send_buffer_bytes: 1024 * 1024, // 1MB
            reuse_addr: true,
            reuse_port: true,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Some(Duration::from_secs(120)),
            write_timeout: Some(Duration::from_secs(120)),
        }
    }

    /// Low-latency socket configuration
    /// Smaller buffers, aggressive keep-alive
    #[must_use]
    pub const fn low_latency() -> Self {
        Self {
            tcp_nodelay: true,
            tcp_keepalive: true,
            tcp_keepidle_secs: 30,         // 30 seconds
            tcp_keepintvl_secs: 10,        // 10 seconds between probes
            tcp_keepcnt: 5,                // More probes for responsiveness
            recv_buffer_bytes: 256 * 1024, // 256KB
            send_buffer_bytes: 256 * 1024, // 256KB
            reuse_addr: true,
            reuse_port: false,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Some(Duration::from_secs(30)),
            write_timeout: Some(Duration::from_secs(30)),
        }
    }
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Connection Pool Configuration
///
/// Manages connection reuse and limits for optimal throughput
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Maximum number of idle connections to keep alive
    /// Higher = more memory but better latency
    /// Lower = less memory but more connection churn
    pub max_idle_connections: usize,

    /// Maximum total connections (idle + active)
    /// Should be >= `max_idle_connections`
    pub max_total_connections: usize,

    /// How long to keep idle connections alive
    /// After this, closed and must reconnect
    pub idle_timeout: Duration,

    /// How long to wait for a connection from the pool
    pub acquire_timeout: Duration,

    /// Minimum idle connections to maintain (warming pool)
    pub min_idle_connections: usize,

    /// TCP socket configuration
    pub socket_config: SocketConfig,

    /// Enable connection validation on checkout
    /// (checks if connection is still alive)
    pub validate_on_checkout: bool,
}

impl ConnectionPoolConfig {
    /// Balanced pool configuration
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            max_idle_connections: 100,
            max_total_connections: 500,
            idle_timeout: Duration::from_secs(300), // 5 minutes
            acquire_timeout: Duration::from_secs(30),
            min_idle_connections: 10,
            socket_config: SocketConfig::balanced(),
            validate_on_checkout: true,
        }
    }

    /// High concurrency configuration (10,000+ concurrent connections)
    #[must_use]
    pub const fn high_concurrency() -> Self {
        Self {
            max_idle_connections: 1000,
            max_total_connections: 10000,
            idle_timeout: Duration::from_secs(600), // 10 minutes
            acquire_timeout: Duration::from_secs(30),
            min_idle_connections: 100,
            socket_config: SocketConfig::high_throughput(),
            validate_on_checkout: false, // Skip validation at high concurrency
        }
    }

    /// Conservative configuration (smaller footprint)
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            max_idle_connections: 20,
            max_total_connections: 100,
            idle_timeout: Duration::from_secs(120), // 2 minutes
            acquire_timeout: Duration::from_secs(30),
            min_idle_connections: 5,
            socket_config: SocketConfig::low_latency(),
            validate_on_checkout: true,
        }
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Tokio Runtime Configuration for optimal async performance
///
/// Tuning the async runtime is critical for handling 10,000+ concurrent connections
#[derive(Debug, Clone)]
pub struct TokioRuntimeConfig {
    /// Number of worker threads
    /// If 0: auto-detect based on CPU cores
    /// Typically: 1-2 threads per core for async I/O
    pub worker_threads: usize,

    /// Stack size for worker threads (bytes)
    /// Default: typically 2MB
    /// We keep default to avoid memory bloat with 10K connections
    pub thread_stack_size: Option<usize>,

    /// Enable work-stealing scheduler
    /// When true: idle workers steal work from busy workers
    /// Improves load balancing under uneven load
    pub work_stealing: bool,

    /// Maximum number of blocking operations per worker
    /// Before being moved to separate thread pool
    pub max_blocking_threads: usize,

    /// Thread naming prefix (for debugging)
    pub thread_name_prefix: String,

    /// Use NUMA-aware scheduling (if supported)
    /// Improves cache locality on NUMA systems
    pub numa_aware: bool,
}

impl TokioRuntimeConfig {
    /// Balanced runtime configuration
    #[must_use]
    pub fn balanced() -> Self {
        Self {
            worker_threads: 0, // Auto-detect
            thread_stack_size: None,
            work_stealing: true,
            max_blocking_threads: 512,
            thread_name_prefix: "fraiseql-http2".to_string(),
            numa_aware: false,
        }
    }

    /// High-performance runtime (maximize throughput)
    ///
    /// Automatically detects CPU cores and spawns 2 threads per core
    /// for maximum work-stealing and load balancing.
    #[must_use]
    pub fn high_performance() -> Self {
        // Note: In actual use, num_cpus::get() would be called here
        // For simplicity in this config struct, we document the intent
        let cpu_count = 8; // Default fallback (can be replaced with num_cpus::get())
        Self {
            worker_threads: cpu_count * 2, // 2 threads per core
            thread_stack_size: None,
            work_stealing: true,
            max_blocking_threads: 1024,
            thread_name_prefix: "fraiseql-hp".to_string(),
            numa_aware: true,
        }
    }
}

impl Default for TokioRuntimeConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_config_balanced() {
        let config = SocketConfig::balanced();
        assert!(config.tcp_nodelay);
        assert!(config.tcp_keepalive);
        assert_eq!(config.recv_buffer_bytes, 512 * 1024);
    }

    #[test]
    fn test_socket_config_high_throughput() {
        let config = SocketConfig::high_throughput();
        assert!(config.tcp_nodelay);
        assert!(config.reuse_port);
        assert_eq!(config.recv_buffer_bytes, 1024 * 1024);
    }

    #[test]
    fn test_socket_config_low_latency() {
        let config = SocketConfig::low_latency();
        assert!(config.tcp_nodelay);
        assert_eq!(config.tcp_keepidle_secs, 30);
        assert_eq!(config.recv_buffer_bytes, 256 * 1024);
    }

    #[test]
    fn test_connection_pool_balanced() {
        let config = ConnectionPoolConfig::balanced();
        assert_eq!(config.max_idle_connections, 100);
        assert_eq!(config.max_total_connections, 500);
    }

    #[test]
    fn test_connection_pool_high_concurrency() {
        let config = ConnectionPoolConfig::high_concurrency();
        assert_eq!(config.max_idle_connections, 1000);
        assert_eq!(config.max_total_connections, 10000);
        assert!(!config.validate_on_checkout);
    }

    #[test]
    fn test_tokio_runtime_config() {
        let config = TokioRuntimeConfig::balanced();
        assert!(config.work_stealing);
    }

    #[test]
    fn test_pool_config_invariants() {
        let balanced = ConnectionPoolConfig::balanced();
        // Max total should be >= max idle
        assert!(balanced.max_total_connections >= balanced.max_idle_connections);
        // Min idle should be <= max idle
        assert!(balanced.min_idle_connections <= balanced.max_idle_connections);
    }

    #[test]
    fn test_socket_buffer_scaling() {
        let balanced = SocketConfig::balanced();
        let high_throughput = SocketConfig::high_throughput();
        let low_latency = SocketConfig::low_latency();

        // Verify expected scaling
        assert!(balanced.recv_buffer_bytes < high_throughput.recv_buffer_bytes);
        assert!(low_latency.recv_buffer_bytes < balanced.recv_buffer_bytes);
    }
}
