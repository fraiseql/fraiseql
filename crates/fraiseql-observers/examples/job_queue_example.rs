//! Example demonstrating asynchronous job queue execution
//!
//! This example shows:
//! 1. Configuring the job queue system
//! 2. Creating and enqueuing jobs
//! 3. Running a worker to process jobs
//! 4. Monitoring with Prometheus metrics
//!
//! # Requirements
//!
//! - Redis running on `redis://localhost:6379`
//! - Features: `queue`, `metrics`, and `testing`
//!
//! # Running
//!
//! ```bash
//! # Terminal 1: Start the worker
//! cargo run --example job_queue_example --features "queue,metrics,testing"
//!
//! # Terminal 2 (while worker is running): Query metrics
//! curl http://localhost:9090/metrics 2>/dev/null | grep fraiseql_observer_job
//! ```

#[cfg(all(feature = "queue", feature = "metrics", feature = "testing"))]
#[tokio::main]
async fn main() -> fraiseql_observers::Result<()> {
    use fraiseql_observers::{
        config::{
            ActionConfig, BackoffStrategy, JobQueueConfig, ObserverDefinition, ObserverRuntimeConfig,
            PerformanceConfig, RetryConfig, TransportConfig,
        },
        event::{EntityEvent, EventKind},
        executor::ObserverExecutor,
        factory::ExecutorFactory,
        job_queue::executor::JobExecutor,
        matcher::EventMatcher,
        metrics::MetricsRegistry,
        testing::mocks::MockDeadLetterQueue,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    println!("═══════════════════════════════════════════════════════════════");
    println!("FraiseQL Job Queue Example");
    println!("═══════════════════════════════════════════════════════════════");

    // =========================================================================
    // 1. Configuration
    // =========================================================================

    println!("\n[1/5] Setting up configuration...");

    let job_queue_config = JobQueueConfig {
        url: "redis://localhost:6379".to_string(),
        batch_size: 10,
        batch_timeout_secs: 2,
        max_retries: 3,
        worker_concurrency: 5,
        poll_interval_ms: 500,
        initial_delay_ms: 100,
        max_delay_ms: 5000,
    };

    job_queue_config.validate()?;
    println!("✓ Job queue config validated");
    println!("  - URL: {}", job_queue_config.url);
    println!("  - Batch size: {}", job_queue_config.batch_size);
    println!("  - Worker concurrency: {}", job_queue_config.worker_concurrency);
    println!("  - Max retries: {}", job_queue_config.max_retries);

    // =========================================================================
    // 2. Create Job Queue
    // =========================================================================

    println!("\n[2/5] Initializing Redis job queue...");

    let job_queue = ExecutorFactory::build_job_queue(&job_queue_config).await?;
    println!("✓ Job queue connected to Redis");

    // =========================================================================
    // 3. Create Test Observer & Executor
    // =========================================================================

    println!("\n[3/5] Setting up observer executor...");

    // Create a simple observer definition
    let mut observers = HashMap::new();
    observers.insert(
        "user_created_webhook".to_string(),
        ObserverDefinition {
            event_type: "INSERT".to_string(),
            entity: "User".to_string(),
            condition: None,
            actions: vec![ActionConfig::Webhook {
                url: Some("https://webhook.example.com/user-created".to_string()),
                url_env: None,
                headers: Default::default(),
                body_template: None,
            }],
            retry: RetryConfig {
                max_attempts: 3,
                initial_delay_ms: 100,
                max_delay_ms: 5000,
                backoff_strategy: BackoffStrategy::Exponential,
            },
            on_failure: Default::default(),
        },
    );

    let matcher = EventMatcher::build(observers)?;
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = Arc::new(ObserverExecutor::new(matcher, dlq));
    println!("✓ Observer executor created");

    // =========================================================================
    // 4. Queue Test Jobs
    // =========================================================================

    println!("\n[4/5] Queueing test jobs...");

    // Create and queue some test jobs
    let queued_executor = ExecutorFactory::build_with_queue(
        &ObserverRuntimeConfig {
            transport: TransportConfig::default(),
            redis: None,
            clickhouse: None,
            job_queue: Some(job_queue_config.clone()),
            performance: PerformanceConfig::default(),
            observers: HashMap::new(),
            channel_capacity: 1000,
            max_concurrency: 50,
            overflow_policy: fraiseql_observers::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout: "30s".to_string(),
        },
        Arc::new(MockDeadLetterQueue::new()),
    )
    .await?;

    for i in 1..=5 {
        let event = EntityEvent::new(
            EventKind::Created,
            "User".to_string(),
            Uuid::new_v4(),
            json!({
                "id": i,
                "name": format!("User {}", i),
                "email": format!("user{}@example.com", i)
            }),
        );

        let summary = queued_executor.process_event(&event).await?;
        println!(
            "✓ Event {} queued: {} actions queued, {} errors",
            i,
            summary.jobs_queued,
            summary.queueing_errors
        );
    }

    // =========================================================================
    // 5. Initialize Metrics
    // =========================================================================

    println!("\n[5/5] Initializing Prometheus metrics...");

    let _metrics = MetricsRegistry::global().unwrap_or_default();
    println!("✓ Metrics registry initialized");
    println!("  - Use: curl http://localhost:9090/metrics | grep fraiseql_observer_job");

    // =========================================================================
    // Worker Loop
    // =========================================================================

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("Starting job worker (CTRL+C to exit)...");
    println!("═══════════════════════════════════════════════════════════════\n");

    let worker = JobExecutor::new(
        job_queue,
        executor,
        job_queue_config.worker_concurrency,
        job_queue_config.batch_size,
        60, // 60 second timeout per job
    );

    println!("Worker ID: {}", worker.worker_id());
    println!("Configuration:");
    println!("  - Batch size: {}", job_queue_config.batch_size);
    println!("  - Worker concurrency: {}", job_queue_config.worker_concurrency);
    println!("  - Max retries: {}", job_queue_config.max_retries);
    println!("  - Poll interval: {}ms", job_queue_config.poll_interval_ms);
    println!("\nMetrics to monitor:");
    println!("  - fraiseql_observer_job_queued_total");
    println!("  - fraiseql_observer_job_executed_total");
    println!("  - fraiseql_observer_job_failed_total");
    println!("  - fraiseql_observer_job_duration_seconds");
    println!("  - fraiseql_observer_job_retry_attempts_total");
    println!("  - fraiseql_observer_job_queue_depth");
    println!("  - fraiseql_observer_job_dlq_items\n");

    // Run the worker (blocks until error or SIGTERM)
    worker.run().await?;

    println!("\nWorker exited gracefully.");
    Ok(())
}

#[cfg(not(all(feature = "queue", feature = "metrics", feature = "testing")))]
fn main() {
    eprintln!("This example requires features: queue, metrics, testing");
    eprintln!("\nRun with:");
    eprintln!("  cargo run --example job_queue_example --features \"queue,metrics,testing\"");
    std::process::exit(1);
}
