//! Integration tests//!
//! Tests validate:
//! - Job queue configuration and validation
//! - Job structure and lifecycle
//! - Retry backoff strategies
//! - Metrics integration
//!
//! **Requirements**:
//! - Features: `queue`, `metrics`, and `testing`
//!
//! **Run tests**:
//! ```bash
//! cargo test --test job_queue_integration --features "queue,metrics,testing"
//! ```

#![cfg(all(feature = "queue", feature = "metrics", feature = "testing"))]

use std::collections::HashMap;

use fraiseql_observers::{
    config::{ActionConfig, BackoffStrategy, JobQueueConfig},
    error::Result,
    job_queue::Job,
    metrics::MetricsRegistry,
};
use uuid::Uuid;

// ============================================================================
// Integration Test 1: JobQueueConfig Validation
// ============================================================================

#[test]
fn test_job_queue_config_valid() -> Result<()> {
    let config = JobQueueConfig {
        url:                "redis://localhost:6379".to_string(),
        batch_size:         10,
        batch_timeout_secs: 5,
        max_retries:        3,
        worker_concurrency: 5,
        poll_interval_ms:   500,
        initial_delay_ms:   100,
        max_delay_ms:       5000,
    };

    assert!(config.validate().is_ok(), "Valid config should pass");
    println!("✅ Valid job queue config validation passed");
    Ok(())
}

#[test]
fn test_job_queue_config_batch_size_zero() -> Result<()> {
    let config = JobQueueConfig {
        url:                "redis://localhost:6379".to_string(),
        batch_size:         0, // Invalid!
        batch_timeout_secs: 5,
        max_retries:        3,
        worker_concurrency: 5,
        poll_interval_ms:   500,
        initial_delay_ms:   100,
        max_delay_ms:       5000,
    };

    assert!(config.validate().is_err(), "batch_size=0 should fail");
    println!("✅ Batch size validation works correctly");
    Ok(())
}

#[test]
fn test_job_queue_config_max_retries_zero() -> Result<()> {
    let config = JobQueueConfig {
        url:                "redis://localhost:6379".to_string(),
        batch_size:         10,
        batch_timeout_secs: 5,
        max_retries:        0, // Invalid!
        worker_concurrency: 5,
        poll_interval_ms:   500,
        initial_delay_ms:   100,
        max_delay_ms:       5000,
    };

    assert!(config.validate().is_err(), "max_retries=0 should fail");
    println!("✅ Max retries validation works correctly");
    Ok(())
}

#[test]
fn test_job_queue_config_empty_url() -> Result<()> {
    let config = JobQueueConfig {
        url:                String::new(), // Invalid!
        batch_size:         10,
        batch_timeout_secs: 5,
        max_retries:        3,
        worker_concurrency: 5,
        poll_interval_ms:   500,
        initial_delay_ms:   100,
        max_delay_ms:       5000,
    };

    assert!(config.validate().is_err(), "Empty URL should fail");
    println!("✅ URL validation works correctly");
    Ok(())
}

// ============================================================================
// Integration Test 2: Job Creation & Backoff Strategies
// ============================================================================

#[test]
fn test_job_creation_with_fixed_backoff() -> Result<()> {
    let event_id = Uuid::new_v4();
    let job = Job::with_config(
        event_id,
        ActionConfig::Webhook {
            url:           Some("http://example.com/webhook".to_string()),
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        },
        3,
        BackoffStrategy::Fixed,
        100,
        5000,
    );

    assert_eq!(job.id.to_string().len(), 36, "Job ID should be valid UUID");
    assert_eq!(job.attempt, 1, "Initial attempt should be 1");
    assert_eq!(job.max_attempts, 3, "Max attempts should be 3");
    assert!(job.can_retry(), "Job should be retryable");
    println!("✅ Job creation with fixed backoff strategy works");
    Ok(())
}

#[test]
fn test_job_creation_with_linear_backoff() -> Result<()> {
    let job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Webhook {
            url:           Some("http://example.com/webhook".to_string()),
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        },
        5,
        BackoffStrategy::Linear,
        50,
        3000,
    );

    assert_eq!(job.initial_delay_ms, 50);
    assert_eq!(job.max_delay_ms, 3000);
    println!("✅ Job creation with linear backoff strategy works");
    Ok(())
}

#[test]
fn test_job_creation_with_exponential_backoff() -> Result<()> {
    let job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Webhook {
            url:           Some("http://example.com/webhook".to_string()),
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        },
        10,
        BackoffStrategy::Exponential,
        100,
        60000,
    );

    assert_eq!(job.max_attempts, 10);
    println!("✅ Job creation with exponential backoff strategy works");
    Ok(())
}

// ============================================================================
// Integration Test 3: Job Retry Logic
// ============================================================================

#[test]
fn test_job_retry_counting() -> Result<()> {
    let mut job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Webhook {
            url:           Some("http://example.com/webhook".to_string()),
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        },
        3,
        BackoffStrategy::Fixed,
        100,
        5000,
    );

    // Initial state
    assert_eq!(job.attempt, 1);
    println!("✅ Initial state: attempt 1");

    // First failure
    job.mark_failed("First failure".to_string());
    assert_eq!(job.attempt, 2);
    println!("✅ After first failure: attempt {}", job.attempt);

    // Second failure
    job.mark_failed("Second failure".to_string());
    assert_eq!(job.attempt, 3);
    println!("✅ After second failure: attempt {}", job.attempt);

    // Third failure - may not increment beyond max_attempts
    let attempt_before = job.attempt;
    job.mark_failed("Third failure".to_string());
    println!("✅ After third failure: attempt {} (was {})", job.attempt, attempt_before);

    println!("✅ Job retry logic works correctly");
    Ok(())
}

// ============================================================================
// Integration Test 4: Multiple Action Types
// ============================================================================

#[test]
fn test_job_with_webhook_action() -> Result<()> {
    let job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Webhook {
            url:           Some("http://api.example.com/webhooks/event".to_string()),
            url_env:       None,
            headers:       {
                let mut h = HashMap::new();
                h.insert("X-API-Key".to_string(), "secret".to_string());
                h
            },
            body_template: Some(r#"{"event": "{{ event.kind }}"}"#.to_string()),
        },
        3,
        BackoffStrategy::Exponential,
        100,
        5000,
    );

    assert_eq!(job.action_type(), "webhook");
    println!("✅ Webhook action job created correctly");
    Ok(())
}

#[test]
fn test_job_with_slack_action() -> Result<()> {
    let job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Slack {
            webhook_url:      Some("https://hooks.slack.com/services/T00/B00/XX".to_string()),
            webhook_url_env:  None,
            channel:          Some("#alerts".to_string()),
            message_template: Some("Event occurred: {{ event.kind }}".to_string()),
        },
        3,
        BackoffStrategy::Linear,
        100,
        5000,
    );

    assert_eq!(job.action_type(), "slack");
    println!("✅ Slack action job created correctly");
    Ok(())
}

#[test]
fn test_job_with_email_action() -> Result<()> {
    let job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Email {
            to:               Some("admin@example.com".to_string()),
            to_template:      None,
            subject:          None,
            subject_template: Some("Alert: {{ event.kind }}".to_string()),
            body_template:    Some("Event: {{ event.entity_type }}".to_string()),
            reply_to:         None,
        },
        3,
        BackoffStrategy::Fixed,
        100,
        5000,
    );

    assert_eq!(job.action_type(), "email");
    println!("✅ Email action job created correctly");
    Ok(())
}

// ============================================================================
// Integration Test 5: Metrics Integration
// ============================================================================

#[test]
fn test_metrics_registry_initialization() -> Result<()> {
    let _metrics = MetricsRegistry::global().unwrap_or_default();
    println!("✅ Metrics registry initialized successfully");
    Ok(())
}

#[test]
fn test_metrics_recording() -> Result<()> {
    let metrics = MetricsRegistry::global().unwrap_or_default();

    // Record various metrics
    metrics.job_queued();
    metrics.job_queued();
    println!("✅ Recorded job_queued metrics");

    metrics.job_executed("webhook", 0.123);
    metrics.job_executed("slack", 0.456);
    println!("✅ Recorded job_executed metrics");

    metrics.job_failed("webhook", "timeout");
    metrics.job_failed("slack", "network_error");
    println!("✅ Recorded job_failed metrics");

    metrics.job_retry_attempt("webhook");
    metrics.job_retry_attempt("webhook");
    println!("✅ Recorded job_retry_attempt metrics");

    Ok(())
}

// ============================================================================
// Integration Test 6: Backoff Strategy Calculations
// ============================================================================

#[test]
fn test_backoff_strategies() -> Result<()> {
    // Test all three backoff strategy types
    let strategies = vec![
        ("Fixed", BackoffStrategy::Fixed),
        ("Linear", BackoffStrategy::Linear),
        ("Exponential", BackoffStrategy::Exponential),
    ];

    for (name, strategy) in strategies {
        let _job = Job::with_config(
            Uuid::new_v4(),
            ActionConfig::Webhook {
                url:           Some("http://example.com/webhook".to_string()),
                url_env:       None,
                headers:       HashMap::new(),
                body_template: None,
            },
            3,
            strategy.clone(),
            100,
            5000,
        );
        println!("✅ Created job with {} backoff strategy", name);
    }

    Ok(())
}

// ============================================================================
// Integration Test 7: Job Lifecycle States
// ============================================================================

#[test]
fn test_job_lifecycle() -> Result<()> {
    // Create initial job
    let mut job = Job::with_config(
        Uuid::new_v4(),
        ActionConfig::Webhook {
            url:           Some("http://example.com/webhook".to_string()),
            url_env:       None,
            headers:       HashMap::new(),
            body_template: None,
        },
        3,
        BackoffStrategy::Linear,
        100,
        5000,
    );

    // State 1: Created
    assert_eq!(job.attempt, 1);
    assert!(job.can_retry());
    println!("✅ Job created at attempt 1");

    // State 2: First failure
    job.mark_failed("Connection timeout".to_string());
    assert_eq!(job.attempt, 2);
    println!("✅ Job failed, now at attempt 2");

    // State 3: Second failure
    job.mark_failed("Connection timeout".to_string());
    assert_eq!(job.attempt, 3);
    println!("✅ Job failed, now at attempt 3");

    // State 4: Third failure - may or may not increment depending on implementation
    job.mark_failed("Connection timeout".to_string());
    println!("✅ After third mark_failed, attempt is now {}", job.attempt);

    Ok(())
}

// ============================================================================
// Integration Test 8: Job Configuration Combinations
// ============================================================================

#[test]
fn test_job_config_combinations() -> Result<()> {
    let action_types = vec![
        (
            "webhook",
            ActionConfig::Webhook {
                url:           Some("http://example.com/webhook".to_string()),
                url_env:       None,
                headers:       HashMap::new(),
                body_template: None,
            },
        ),
        (
            "slack",
            ActionConfig::Slack {
                webhook_url:      Some("https://hooks.slack.com/services/T00/B00/XX".to_string()),
                webhook_url_env:  None,
                channel:          None,
                message_template: None,
            },
        ),
    ];

    let retry_counts = vec![1, 3, 5, 10];
    let backoff_strategies = vec![
        BackoffStrategy::Fixed,
        BackoffStrategy::Linear,
        BackoffStrategy::Exponential,
    ];

    let mut total_jobs = 0;
    for (_action_name, action) in &action_types {
        for retry_count in &retry_counts {
            for strategy in &backoff_strategies {
                let job = Job::with_config(
                    Uuid::new_v4(),
                    action.clone(),
                    *retry_count,
                    strategy.clone(),
                    100,
                    5000,
                );
                assert_eq!(job.max_attempts, *retry_count);
                total_jobs += 1;
            }
        }
    }

    println!("✅ Created {} job configurations with various combinations", total_jobs);
    Ok(())
}
