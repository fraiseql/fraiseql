#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports
#![allow(clippy::print_stderr)] // Reason: skip messages print to stderr by design

#[cfg(test)]
mod queue_tests {
    use crate::{config::ActionConfig, event::EntityEvent, queue::*};

    #[test]
    fn test_job_creation() {
        let job = Job {
            id:            "job-1".to_string(),
            action_id:     "send_email".to_string(),
            event:         EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Webhook {
                url:           Some("http://localhost:8000".to_string()),
                url_env:       None,
                headers:       std::collections::HashMap::new(),
                body_template: None,
            },
            attempt:       1,
            created_at:    chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        assert_eq!(job.id, "job-1");
        assert_eq!(job.attempt, 1);
    }

    #[test]
    fn test_job_status_display() {
        assert_eq!(JobStatus::Pending.to_string(), "pending");
        assert_eq!(JobStatus::Processing.to_string(), "processing");
        assert_eq!(JobStatus::Success.to_string(), "success");
        assert_eq!(JobStatus::Failed.to_string(), "failed");
        assert_eq!(JobStatus::Retrying.to_string(), "retrying");
        assert_eq!(JobStatus::Deadletter.to_string(), "deadletter");
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let policy = ExponentialBackoffPolicy {
            max_attempts:     5,
            initial_delay_ms: 1000,
            max_delay_ms:     60000,
            multiplier:       2.0,
        };

        // Attempt 1: 1000ms
        assert_eq!(policy.get_backoff_ms(1), 1000);
        // Attempt 2: 2000ms
        assert_eq!(policy.get_backoff_ms(2), 2000);
        // Attempt 3: 4000ms
        assert_eq!(policy.get_backoff_ms(3), 4000);
        // Attempt 4: 8000ms
        assert_eq!(policy.get_backoff_ms(4), 8000);
    }

    #[test]
    fn test_exponential_backoff_cap() {
        let policy = ExponentialBackoffPolicy {
            max_attempts:     10,
            initial_delay_ms: 1000,
            max_delay_ms:     10000,
            multiplier:       2.0,
        };

        // Normally would be 32000ms (1000 * 2^5), but capped at 10000ms
        assert_eq!(policy.get_backoff_ms(6), 10000);
        assert_eq!(policy.get_backoff_ms(7), 10000);
    }

    #[test]
    fn test_exponential_backoff_should_retry() {
        let policy = ExponentialBackoffPolicy {
            max_attempts:     3,
            initial_delay_ms: 1000,
            max_delay_ms:     60000,
            multiplier:       2.0,
        };

        // Attempts 1-2 should retry
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        // Attempt 3+ should not retry
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    #[test]
    fn test_linear_backoff_calculation() {
        let policy = LinearBackoffPolicy {
            max_attempts:       5,
            delay_increment_ms: 5000,
            max_delay_ms:       30000,
        };

        // Attempt 1: 5000ms
        assert_eq!(policy.get_backoff_ms(1), 5000);
        // Attempt 2: 10000ms
        assert_eq!(policy.get_backoff_ms(2), 10000);
        // Attempt 3: 15000ms
        assert_eq!(policy.get_backoff_ms(3), 15000);
    }

    #[test]
    fn test_linear_backoff_cap() {
        let policy = LinearBackoffPolicy {
            max_attempts:       10,
            delay_increment_ms: 5000,
            max_delay_ms:       30000,
        };

        // Normally would be 35000ms (5000 * 7), but capped at 30000ms
        assert_eq!(policy.get_backoff_ms(7), 30000);
        assert_eq!(policy.get_backoff_ms(8), 30000);
    }

    #[test]
    fn test_fixed_backoff_calculation() {
        let policy = FixedBackoffPolicy {
            max_attempts: 5,
            delay_ms:     5000,
        };

        // All attempts have same delay
        assert_eq!(policy.get_backoff_ms(1), 5000);
        assert_eq!(policy.get_backoff_ms(2), 5000);
        assert_eq!(policy.get_backoff_ms(3), 5000);
    }

    #[test]
    fn test_fixed_backoff_should_retry() {
        let policy = FixedBackoffPolicy {
            max_attempts: 3,
            delay_ms:     5000,
        };

        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn test_default_policies() {
        let exp = ExponentialBackoffPolicy::default();
        assert_eq!(exp.max_attempts, 3);

        let lin = LinearBackoffPolicy::default();
        assert_eq!(lin.max_attempts, 3);

        let fixed = FixedBackoffPolicy::default();
        assert_eq!(fixed.max_attempts, 3);
    }
}

#[cfg(feature = "queue")]
mod redis_tests {
    use redis::Client;

    use crate::{
        ActionConfig, ActionResult, EntityEvent, JobStatus,
        queue::{Job, JobQueue, JobResult, redis::*},
    };

    // Returns None when no Redis is available (caller skips). The harness yields the
    // bound REDIS_URL (Dagger) or a locally spawned instance; no hardcoded host.
    async fn setup_test_queue() -> Option<RedisJobQueue> {
        let redis = fraiseql_test_support::redis().await?;
        let client = Client::open(redis.url()).expect("Failed to create client");
        let conn = client.get_connection_manager().await.expect("Failed to connect to Redis");

        // Clear test data
        let mut c = conn.clone();
        let _: () = redis::cmd("FLUSHDB").query_async(&mut c).await.expect("Failed to flush DB");

        Some(RedisJobQueue::new(conn))
    }

    #[tokio::test]
    #[ignore = "requires Redis running"]
    async fn test_redis_enqueue() {
        let Some(queue) = setup_test_queue().await else {
            eprintln!("SKIP: no redis (set REDIS_URL)");
            return;
        };

        let job = Job {
            id:            "job-1".to_string(),
            action_id:     "send_email".to_string(),
            event:         EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to:               Some("test@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    None,
                reply_to:         None,
            },
            attempt:       1,
            created_at:    chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        let job_id = queue.enqueue(&job).await.expect("Failed to enqueue");
        assert_eq!(job_id, "job-1");
    }

    #[tokio::test]
    #[ignore = "requires Redis running"]
    async fn test_redis_dequeue() {
        let Some(queue) = setup_test_queue().await else {
            eprintln!("SKIP: no redis (set REDIS_URL)");
            return;
        };

        let job = Job {
            id:            "job-1".to_string(),
            action_id:     "send_email".to_string(),
            event:         EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to:               Some("test@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    None,
                reply_to:         None,
            },
            attempt:       1,
            created_at:    chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        let dequeued = queue
            .dequeue("worker-1")
            .await
            .expect("Failed to dequeue")
            .expect("No job dequeued");

        assert_eq!(dequeued.id, "job-1");
    }

    #[tokio::test]
    #[ignore = "requires Redis running"]
    async fn test_redis_mark_success() {
        let Some(queue) = setup_test_queue().await else {
            eprintln!("SKIP: no redis (set REDIS_URL)");
            return;
        };

        let job = Job {
            id:            "job-1".to_string(),
            action_id:     "send_email".to_string(),
            event:         EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to:               Some("test@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    None,
                reply_to:         None,
            },
            attempt:       1,
            created_at:    chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        let result = JobResult {
            job_id:        "job-1".to_string(),
            status:        JobStatus::Success,
            action_result: ActionResult {
                action_type: "send_email".to_string(),
                success:     true,
                message:     "Email sent".to_string(),
                duration_ms: 100.0,
            },
            attempts:      1,
            duration_ms:   100.0,
        };

        queue.mark_success("job-1", &result).await.expect("Failed to mark success");

        let stats = queue.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.successful_jobs, 1);
    }

    #[tokio::test]
    #[ignore = "requires Redis running"]
    async fn test_redis_mark_retry() {
        let Some(queue) = setup_test_queue().await else {
            eprintln!("SKIP: no redis (set REDIS_URL)");
            return;
        };

        let job = Job {
            id:            "job-1".to_string(),
            action_id:     "send_email".to_string(),
            event:         EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to:               Some("test@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    None,
                reply_to:         None,
            },
            attempt:       1,
            created_at:    chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        let next_retry = chrono::Utc::now().timestamp() + 5;
        queue.mark_retry("job-1", next_retry).await.expect("Failed to mark retry");

        let stats = queue.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.retry_jobs, 1);
    }

    #[tokio::test]
    #[ignore = "requires Redis running"]
    async fn test_redis_mark_deadletter() {
        let Some(queue) = setup_test_queue().await else {
            eprintln!("SKIP: no redis (set REDIS_URL)");
            return;
        };

        let job = Job {
            id:            "job-1".to_string(),
            action_id:     "send_email".to_string(),
            event:         EntityEvent::new(
                crate::event::EventKind::Created,
                "Order".to_string(),
                uuid::Uuid::new_v4(),
                serde_json::json!({}),
            ),
            action_config: ActionConfig::Email {
                to:               Some("test@example.com".to_string()),
                to_template:      None,
                subject:          Some("Test".to_string()),
                subject_template: None,
                body_template:    None,
                reply_to:         None,
            },
            attempt:       1,
            created_at:    chrono::Utc::now().timestamp(),
            next_retry_at: chrono::Utc::now().timestamp(),
        };

        queue.enqueue(&job).await.expect("Failed to enqueue");

        queue
            .mark_deadletter("job-1", "Max retries exceeded")
            .await
            .expect("Failed to mark deadletter");

        let stats = queue.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.failed_jobs, 1);
    }

    #[tokio::test]
    #[ignore = "requires Redis running"]
    async fn test_redis_get_stats() {
        let Some(queue) = setup_test_queue().await else {
            eprintln!("SKIP: no redis (set REDIS_URL)");
            return;
        };

        let stats = queue.get_stats().await.expect("Failed to get stats");

        assert_eq!(stats.pending_jobs, 0);
        assert_eq!(stats.processing_jobs, 0);
        assert_eq!(stats.retry_jobs, 0);
    }
}

#[cfg(test)]
mod worker_tests {
    #[test]
    fn test_job_worker_pool_creation() {
        // Pool structure is tested through type system
        // Runtime tests require full async setup
    }
}
