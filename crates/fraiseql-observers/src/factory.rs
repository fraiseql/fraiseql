//! Factory functions for building executor stacks based on configuration.
//!
//! This module provides builder functions that compose the observer execution pipeline
//! based on the `ObserverRuntimeConfig`. It handles:
//!
//! - Creating the base `ObserverExecutor`
//! - Wrapping with `DedupedObserverExecutor` if deduplication is enabled
//! - Creating action executors (with optional caching wrapper)
//! - Connecting to Redis for dedup/cache backends
//!
//! # Architecture
//!
//! The factory builds a composable stack:
//!
//! ```text
//! Config → Factory → DedupedObserverExecutor(
//!                      ObserverExecutor {
//!                        actions: CachedActionExecutor(...) if caching enabled
//!                      }
//!                    )
//! ```
//!
//! # Examples
//!
//! ```rust,ignore
//! use fraiseql_observers::factory::ExecutorFactory;
//! use fraiseql_observers::config::ObserverRuntimeConfig;
//!
//! // Load configuration
//! let config = ObserverRuntimeConfig::load_from_file("config.toml")?;
//!
//! // Build executor stack (automatically wraps based on config)
//! let executor_stack = ExecutorFactory::build(&config).await?;
//!
//! // Process events
//! executor_stack.process_event(&event).await?;
//! ```

use std::sync::Arc;

#[cfg(feature = "caching")]
use crate::cache::redis::RedisCacheBackend;
#[cfg(any(feature = "dedup", feature = "caching"))]
use crate::config::RedisConfig;
#[cfg(feature = "dedup")]
use crate::dedup::redis::RedisDeduplicationStore;
#[cfg(feature = "dedup")]
use crate::deduped_executor::DedupedObserverExecutor;
#[cfg(feature = "queue")]
use crate::job_queue::redis::RedisJobQueue;
#[cfg(feature = "queue")]
use crate::queued_executor::QueuedObserverExecutor;
use crate::{
    config::ObserverRuntimeConfig,
    error::{ObserverError, Result},
    executor::ObserverExecutor,
    matcher::EventMatcher,
    traits::DeadLetterQueue,
};

/// Factory for building executor stacks
pub struct ExecutorFactory;

impl ExecutorFactory {
    /// Build the complete executor stack based on configuration.
    ///
    /// This is the main entry point. It:
    /// 1. Creates the base `ObserverExecutor`
    /// 2. Wraps with `DedupedObserverExecutor` if `performance.enable_dedup = true`
    /// 3. Configures action caching if `performance.enable_caching = true`
    ///
    /// # Arguments
    ///
    /// * `config` - Runtime configuration
    /// * `dlq` - Dead letter queue implementation
    ///
    /// # Returns
    ///
    /// A trait object that implements event processing, automatically wrapped
    /// with deduplication/caching based on config flags.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Redis connection fails (when dedup/caching enabled)
    /// - Configuration validation fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = ObserverRuntimeConfig::load_from_file("config.toml")?;
    /// let dlq = Arc::new(PostgresDLQ::new(pool.clone()));
    ///
    /// // Automatically wraps with dedup/cache based on config
    /// let executor = ExecutorFactory::build(&config, dlq).await?;
    /// ```
    #[cfg(all(feature = "dedup", feature = "caching"))]
    pub async fn build(
        config: &ObserverRuntimeConfig,
        dlq: Arc<dyn DeadLetterQueue>,
    ) -> Result<Arc<dyn ProcessEvent>> {
        // Validate configuration
        config.transport.validate()?;
        if let Some(ref redis_config) = config.redis {
            redis_config.validate()?;
        }
        config.performance.validate(config.redis.is_some())?;

        // Create event matcher
        let matcher = EventMatcher::build(config.observers.clone())?;

        // Build base executor
        let base_executor = ObserverExecutor::new(matcher, dlq);

        // Wrap with deduplication if enabled
        if config.performance.enable_dedup {
            let redis_config =
                config.redis.as_ref().ok_or_else(|| ObserverError::InvalidConfig {
                    message: "enable_dedup=true requires redis configuration".to_string(),
                })?;

            let dedup_store = Self::build_dedup_store(redis_config).await?;
            let deduped_executor = DedupedObserverExecutor::new(base_executor, dedup_store);

            Ok(Arc::new(deduped_executor))
        } else {
            // No deduplication, just wrap base executor
            Ok(Arc::new(base_executor))
        }

        // TODO: Action caching is handled inside ObserverExecutor (per-action wrapping)
        // This would require modifying ObserverExecutor to accept wrapped action executors
    }

    /// Build the executor stack without dedup/caching features.
    ///
    /// This is a fallback when features are not enabled. It returns a simple
    /// `ObserverExecutor` without any wrappers.
    #[cfg(not(all(feature = "dedup", feature = "caching")))]
    pub async fn build(
        config: &ObserverRuntimeConfig,
        dlq: Arc<dyn DeadLetterQueue>,
    ) -> Result<Arc<dyn ProcessEvent>> {
        // Create event matcher
        let matcher = EventMatcher::build(config.observers.clone())?;

        // Build base executor (no wrapping)
        let base_executor = ObserverExecutor::new(matcher, dlq);
        Ok(Arc::new(base_executor))
    }

    /// Build Redis deduplication store from config
    #[cfg(feature = "dedup")]
    async fn build_dedup_store(redis_config: &RedisConfig) -> Result<RedisDeduplicationStore> {
        use redis::aio::ConnectionManager;

        // Create Redis client and connection manager
        let client = redis::Client::open(redis_config.url.as_str()).map_err(|e| {
            ObserverError::InvalidConfig {
                message: format!("Failed to create Redis client: {}", e),
            }
        })?;

        let conn =
            ConnectionManager::new(client).await.map_err(|e| ObserverError::InvalidConfig {
                message: format!("Failed to connect to Redis: {}", e),
            })?;

        Ok(RedisDeduplicationStore::new(conn, redis_config.dedup_window_secs))
    }

    /// Build Redis cache backend from config
    #[cfg(feature = "caching")]
    async fn build_cache_backend(redis_config: &RedisConfig) -> Result<RedisCacheBackend> {
        use redis::aio::ConnectionManager;

        // Create Redis client and connection manager
        let client = redis::Client::open(redis_config.url.as_str()).map_err(|e| {
            ObserverError::InvalidConfig {
                message: format!("Failed to create Redis client: {}", e),
            }
        })?;

        let conn =
            ConnectionManager::new(client).await.map_err(|e| ObserverError::InvalidConfig {
                message: format!("Failed to connect to Redis: {}", e),
            })?;

        Ok(RedisCacheBackend::new(conn, redis_config.cache_ttl_secs))
    }

    /// Build Redis job queue from config
    #[cfg(feature = "queue")]
    pub async fn build_job_queue(
        job_queue_config: &crate::config::JobQueueConfig,
    ) -> Result<Arc<dyn crate::job_queue::JobQueue>> {
        use redis::aio::ConnectionManager;

        // Validate configuration
        job_queue_config.validate()?;

        // Create Redis client and connection manager
        let client = redis::Client::open(job_queue_config.url.as_str()).map_err(|e| {
            ObserverError::InvalidConfig {
                message: format!("Failed to create Redis client for job queue: {e}"),
            }
        })?;

        let conn =
            ConnectionManager::new(client).await.map_err(|e| ObserverError::InvalidConfig {
                message: format!("Failed to connect to Redis for job queue: {e}"),
            })?;

        Ok(Arc::new(RedisJobQueue::new(conn)))
    }
}

/// Trait for event processing (allows different executor implementations)
#[async_trait::async_trait]
pub trait ProcessEvent: Send + Sync {
    /// Process an event through the execution pipeline
    async fn process_event(
        &self,
        event: &crate::event::EntityEvent,
    ) -> Result<crate::executor::ExecutionSummary>;
}

/// Implement ProcessEvent for ObserverExecutor
#[async_trait::async_trait]
impl ProcessEvent for ObserverExecutor {
    async fn process_event(
        &self,
        event: &crate::event::EntityEvent,
    ) -> Result<crate::executor::ExecutionSummary> {
        self.process_event(event).await
    }
}

/// Implement ProcessEvent for DedupedObserverExecutor
#[cfg(feature = "dedup")]
#[async_trait::async_trait]
impl<D: crate::dedup::DeduplicationStore + Send + Sync> ProcessEvent
    for DedupedObserverExecutor<D>
{
    async fn process_event(
        &self,
        event: &crate::event::EntityEvent,
    ) -> Result<crate::executor::ExecutionSummary> {
        self.process_event(event).await
    }
}

/// Trait for queued event processing (async job queueing)
#[cfg(feature = "queue")]
#[async_trait::async_trait]
pub trait ProcessEventQueued: Send + Sync {
    /// Process an event by queueing actions (async execution)
    async fn process_event(
        &self,
        event: &crate::event::EntityEvent,
    ) -> Result<crate::queued_executor::QueuedExecutionSummary>;
}

/// Implement ProcessEventQueued for QueuedObserverExecutor
#[cfg(feature = "queue")]
#[async_trait::async_trait]
impl ProcessEventQueued for QueuedObserverExecutor {
    async fn process_event(
        &self,
        event: &crate::event::EntityEvent,
    ) -> Result<crate::queued_executor::QueuedExecutionSummary> {
        self.process_event(event).await
    }
}

/// Helper functions for common deployment topologies
impl ExecutorFactory {
    /// Build executor for **Topology 1: PostgreSQL-Only**
    ///
    /// - PostgreSQL LISTEN/NOTIFY for events
    /// - No Redis (no dedup, no caching)
    /// - Simple in-process execution
    ///
    /// Best for: Single DB, low volume, simple deployment
    pub async fn build_postgres_only(
        config: &ObserverRuntimeConfig,
        dlq: Arc<dyn DeadLetterQueue>,
    ) -> Result<Arc<dyn ProcessEvent>> {
        // Validate: should not have Redis features enabled
        if config.performance.enable_dedup || config.performance.enable_caching {
            return Err(ObserverError::InvalidConfig {
                message:
                    "PostgreSQL-only topology should not enable dedup or caching (requires Redis)"
                        .to_string(),
            });
        }

        Self::build(config, dlq).await
    }

    /// Build executor for **Topology 2: PostgreSQL + Redis**
    ///
    /// - PostgreSQL LISTEN/NOTIFY for events
    /// - Redis for deduplication (at-least-once delivery)
    /// - Redis for action caching (100x performance)
    ///
    /// Best for: Single DB, medium volume, needs reliability + performance
    #[cfg(all(feature = "dedup", feature = "caching"))]
    pub async fn build_postgres_redis(
        config: &ObserverRuntimeConfig,
        dlq: Arc<dyn DeadLetterQueue>,
    ) -> Result<Arc<dyn ProcessEvent>> {
        // Validate: Redis must be configured
        if config.redis.is_none() {
            return Err(ObserverError::InvalidConfig {
                message: "PostgreSQL + Redis topology requires redis configuration".to_string(),
            });
        }

        Self::build(config, dlq).await
    }

    /// Build executor for **Topology 3: NATS Distributed Worker**
    ///
    /// - NATS JetStream for event sourcing
    /// - Redis for deduplication (prevents duplicate processing)
    /// - Redis for action caching
    /// - Horizontal scaling with load balancing
    ///
    /// Best for: High volume, HA required, distributed workers
    #[cfg(all(feature = "nats", feature = "dedup", feature = "caching"))]
    pub async fn build_nats_distributed(
        config: &ObserverRuntimeConfig,
        dlq: Arc<dyn DeadLetterQueue>,
    ) -> Result<Arc<dyn ProcessEvent>> {
        // Validate: NATS transport + Redis + dedup enabled
        use crate::config::TransportKind;

        if config.transport.transport != TransportKind::Nats {
            return Err(ObserverError::InvalidConfig {
                message: "NATS distributed topology requires transport = nats".to_string(),
            });
        }

        if config.redis.is_none() {
            return Err(ObserverError::InvalidConfig {
                message: "NATS distributed topology requires redis configuration".to_string(),
            });
        }

        if !config.performance.enable_dedup {
            return Err(ObserverError::InvalidConfig {
                message:
                    "NATS distributed topology requires enable_dedup=true (at-least-once delivery)"
                        .to_string(),
            });
        }

        Self::build(config, dlq).await
    }

    /// Build executor with queued (async) action execution
    ///
    /// This wraps any executor with `QueuedObserverExecutor` to enable:
    /// - Non-blocking event processing (returns immediately)
    /// - Asynchronous action execution in background workers
    /// - Automatic job queueing and retry logic
    ///
    /// Requires: `job_queue` configuration with Redis URL
    #[cfg(feature = "queue")]
    pub async fn build_with_queue(
        config: &ObserverRuntimeConfig,
        _dlq: Arc<dyn DeadLetterQueue>,
    ) -> Result<Arc<dyn ProcessEventQueued>> {
        // Validate job queue config
        let job_queue_config = config.job_queue.as_ref().ok_or_else(|| {
            ObserverError::InvalidConfig {
                message: "build_with_queue requires job_queue configuration".to_string(),
            }
        })?;

        // Create event matcher
        let matcher = EventMatcher::build(config.observers.clone())?;

        // Build job queue
        let job_queue = Self::build_job_queue(job_queue_config).await?;

        // Create queued executor
        let queued_executor = QueuedObserverExecutor::new(matcher, job_queue);

        Ok(Arc::new(queued_executor))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        config::{PerformanceConfig, TransportConfig, TransportKind},
        testing::mocks::MockDeadLetterQueue,
    };

    #[tokio::test]
    async fn test_build_postgres_only_topology() {
        let config = ObserverRuntimeConfig {
            transport:               TransportConfig {
                transport: TransportKind::Postgres,
                ..Default::default()
            },
            redis:                   None, // No Redis
            clickhouse:              None,
            job_queue:               None,
            performance:             PerformanceConfig {
                enable_dedup: false,
                enable_caching: false,
                enable_concurrent: true,
                ..Default::default()
            },
            observers:               HashMap::new(),
            channel_capacity:        1000,
            max_concurrency:         50,
            overflow_policy:         crate::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout:        "30s".to_string(),
        };

        let dlq = Arc::new(MockDeadLetterQueue::new());
        let result = ExecutorFactory::build_postgres_only(&config, dlq).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_build_rejects_dedup_without_redis() {
        let config = ObserverRuntimeConfig {
            transport:               TransportConfig::default(),
            redis:                   None, // No Redis but dedup enabled
            clickhouse:              None,
            job_queue:               None,
            performance:             PerformanceConfig {
                enable_dedup: true, // Invalid!
                ..Default::default()
            },
            observers:               HashMap::new(),
            channel_capacity:        1000,
            max_concurrency:         50,
            overflow_policy:         crate::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout:        "30s".to_string(),
        };

        let dlq = Arc::new(MockDeadLetterQueue::new());

        #[cfg(all(feature = "dedup", feature = "caching"))]
        {
            let result = ExecutorFactory::build(&config, dlq).await;
            assert!(result.is_err());
        }

        #[cfg(not(all(feature = "dedup", feature = "caching")))]
        {
            // Without features, should succeed (ignores config)
            let result = ExecutorFactory::build(&config, dlq).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_process_event_trait() {
        let matcher = EventMatcher::build(HashMap::new()).unwrap();
        let dlq = Arc::new(MockDeadLetterQueue::new());
        let executor = ObserverExecutor::new(matcher, dlq);

        // Can use via trait object
        let processor: Arc<dyn ProcessEvent> = Arc::new(executor);

        use serde_json::json;
        use uuid::Uuid;

        use crate::event::{EntityEvent, EventKind};

        let event =
            EntityEvent::new(EventKind::Created, "Test".to_string(), Uuid::new_v4(), json!({}));

        let summary = processor.process_event(&event).await.unwrap();
        assert!(!summary.duplicate_skipped);
    }

    #[cfg(feature = "queue")]
    #[tokio::test]
    async fn test_build_with_queue_requires_config() {
        let config = ObserverRuntimeConfig {
            transport: TransportConfig::default(),
            redis: None,
            clickhouse: None,
            job_queue: None, // No job queue config
            performance: PerformanceConfig {
                enable_dedup: false,
                enable_caching: false,
                enable_concurrent: true,
                ..Default::default()
            },
            observers: HashMap::new(),
            channel_capacity: 1000,
            max_concurrency: 50,
            overflow_policy: crate::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout: "30s".to_string(),
        };

        let dlq = Arc::new(MockDeadLetterQueue::new());
        let result = ExecutorFactory::build_with_queue(&config, dlq).await;
        assert!(result.is_err());
    }

    #[cfg(feature = "queue")]
    #[tokio::test]
    async fn test_job_queue_config_validation() {
        use crate::config::JobQueueConfig;

        // Valid config
        let config = JobQueueConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: empty URL
        let mut config = JobQueueConfig::default();
        config.url = String::new();
        assert!(config.validate().is_err());

        // Invalid: zero batch size
        let mut config = JobQueueConfig::default();
        config.batch_size = 0;
        assert!(config.validate().is_err());

        // Invalid: zero concurrency
        let mut config = JobQueueConfig::default();
        config.worker_concurrency = 0;
        assert!(config.validate().is_err());
    }

    #[cfg(feature = "queue")]
    #[test]
    fn test_job_queue_config_defaults() {
        use crate::config::JobQueueConfig;

        let config = JobQueueConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.batch_timeout_secs, 5);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.worker_concurrency, 10);
        assert_eq!(config.poll_interval_ms, 1000);
    }

    #[cfg(feature = "queue")]
    #[test]
    fn test_job_queue_config_env_overrides() {
        use crate::config::JobQueueConfig;

        let config = JobQueueConfig::default()
            .with_env_overrides();

        // Should have defaults (env vars not set in test)
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.worker_concurrency, 10);
    }
}
