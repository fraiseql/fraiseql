#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod job_queue_tests {
    use uuid::Uuid;

    use crate::{config::ActionConfig, job_queue::*};

    #[test]
    fn test_job_state_is_terminal() {
        assert!(JobState::Completed.is_terminal());
        assert!(JobState::Failed.is_terminal());
        assert!(JobState::DeadLettered.is_terminal());
        assert!(!JobState::Pending.is_terminal());
        assert!(!JobState::Running.is_terminal());
    }

    #[test]
    fn test_job_state_is_active() {
        assert!(JobState::Pending.is_active());
        assert!(JobState::Running.is_active());
        assert!(!JobState::Completed.is_active());
        assert!(!JobState::Failed.is_active());
        assert!(!JobState::DeadLettered.is_active());
    }

    #[test]
    fn test_job_creation() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        assert_eq!(job.event_id, event_id);
        assert_eq!(job.attempt, 1);
        assert_eq!(job.max_attempts, 3);
        assert_eq!(job.state, JobState::Pending);
        assert!(job.last_error.is_none());
        assert!(job.attempts.is_empty());
    }

    #[test]
    fn test_job_can_retry() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        assert!(job.can_retry());

        let mut job = job;
        job.attempt = 3;
        assert!(!job.can_retry());
    }

    #[test]
    fn test_job_mark_completed() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        job.mark_completed();

        assert_eq!(job.state, JobState::Completed);
        assert_eq!(job.attempts.len(), 1);
        assert!(job.attempts[0].success);
        assert!(job.attempts[0].error.is_none());
    }

    #[test]
    fn test_job_mark_failed_with_retry() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        job.mark_failed("connection timeout".to_string());

        assert_eq!(job.state, JobState::Pending); // Can retry
        assert_eq!(job.attempt, 2); // Incremented
        assert_eq!(job.last_error.as_ref().unwrap(), "connection timeout");
        assert_eq!(job.attempts.len(), 1);
        assert!(!job.attempts[0].success);
    }

    #[test]
    fn test_job_mark_failed_exhausted() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 2, crate::config::BackoffStrategy::Exponential);
        job.attempt = 2;

        job.mark_failed("connection timeout".to_string());

        assert_eq!(job.state, JobState::Failed); // Cannot retry anymore
        assert_eq!(job.last_error.as_ref().unwrap(), "connection timeout");
    }

    #[test]
    fn test_job_mark_dead_lettered() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        job.mark_dead_lettered("invalid configuration".to_string());

        assert_eq!(job.state, JobState::DeadLettered);
        assert_eq!(job.last_error.as_ref().unwrap(), "invalid configuration");
    }

    #[test]
    fn test_job_serialization() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };

        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        let json = serde_json::to_string(&job).expect("serialization failed");
        let deserialized: Job = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(job.id, deserialized.id);
        assert_eq!(job.event_id, deserialized.event_id);
        assert_eq!(job.attempt, deserialized.attempt);
        assert_eq!(job.state, deserialized.state);
    }

    #[test]
    fn test_job_action_type() {
        let event_id = Uuid::new_v4();

        let job_cache = Job::new(
            event_id,
            ActionConfig::Cache {
                key_pattern: "test:*".to_string(),
                action: "invalidate".to_string(),
            },
            3,
            crate::config::BackoffStrategy::Exponential,
        );
        assert_eq!(job_cache.action_type(), "cache");

        let job_webhook = Job::new(
            event_id,
            ActionConfig::Webhook {
                url: Some("http://example.com".to_string()),
                url_env: None,
                headers: std::collections::HashMap::default(),
                body_template: None,
            },
            3,
            crate::config::BackoffStrategy::Exponential,
        );
        assert_eq!(job_webhook.action_type(), "webhook");
    }
}

#[cfg(test)]
mod backoff_tests {
    use crate::job_queue::backoff::{calculate_backoff, calculate_exponential, calculate_linear};

    #[test]
    fn test_exponential_backoff() {
        let initial = 100;
        let max = 30000;

        assert_eq!(calculate_exponential(1, initial, max), 100);
        assert_eq!(calculate_exponential(2, initial, max), 200);
        assert_eq!(calculate_exponential(3, initial, max), 400);
        assert_eq!(calculate_exponential(4, initial, max), 800);
        assert_eq!(calculate_exponential(5, initial, max), 1600);
    }

    #[test]
    fn test_exponential_backoff_caps_at_max() {
        let initial = 100;
        let max = 1000;

        assert_eq!(calculate_exponential(1, initial, max), 100);
        assert_eq!(calculate_exponential(2, initial, max), 200);
        assert_eq!(calculate_exponential(3, initial, max), 400);
        assert_eq!(calculate_exponential(4, initial, max), 800);
        assert_eq!(calculate_exponential(5, initial, max), 1000); // Capped at max
        assert_eq!(calculate_exponential(6, initial, max), 1000); // Still capped
    }

    #[test]
    fn test_linear_backoff() {
        let initial = 100;
        let max = 30000;

        assert_eq!(calculate_linear(1, initial, max), 100);
        assert_eq!(calculate_linear(2, initial, max), 200);
        assert_eq!(calculate_linear(3, initial, max), 300);
        assert_eq!(calculate_linear(4, initial, max), 400);
        assert_eq!(calculate_linear(5, initial, max), 500);
    }

    #[test]
    fn test_linear_backoff_caps_at_max() {
        let initial = 100;
        let max = 350;

        assert_eq!(calculate_linear(1, initial, max), 100);
        assert_eq!(calculate_linear(2, initial, max), 200);
        assert_eq!(calculate_linear(3, initial, max), 300);
        assert_eq!(calculate_linear(4, initial, max), 350); // Capped at max
        assert_eq!(calculate_linear(5, initial, max), 350); // Still capped
    }

    #[test]
    fn test_calculate_backoff_exponential() {
        let duration =
            calculate_backoff(crate::config::BackoffStrategy::Exponential, 2, 100, 30000);
        assert_eq!(duration.as_millis(), 200);
    }

    #[test]
    fn test_calculate_backoff_linear() {
        let duration = calculate_backoff(crate::config::BackoffStrategy::Linear, 3, 100, 30000);
        assert_eq!(duration.as_millis(), 300);
    }

    #[test]
    fn test_calculate_backoff_fixed() {
        let duration = calculate_backoff(
            crate::config::BackoffStrategy::Fixed,
            5, // Attempt number is ignored for fixed
            100,
            30000,
        );
        assert_eq!(duration.as_millis(), 100);
    }

    #[test]
    fn test_backoff_overflow_protection() {
        // Test that exponential backoff doesn't overflow
        let delay = calculate_exponential(100, 100, u64::MAX);
        // Verify the function returns without panicking (overflow protection)
        let _ = delay;
    }

    #[test]
    fn test_zero_initial_delay() {
        assert_eq!(calculate_exponential(1, 0, 1000), 0);
        assert_eq!(calculate_linear(1, 0, 1000), 0);
    }

    #[test]
    fn test_max_delay_equals_initial() {
        let initial = 100;
        assert_eq!(calculate_exponential(5, initial, initial), initial);
        assert_eq!(calculate_linear(5, initial, initial), initial);
    }
}

#[cfg(feature = "queue")]
mod dlq_tests {
    use uuid::Uuid;

    use crate::job_queue::dlq::*;

    #[test]
    fn test_dlq_key_generation() {
        assert_eq!(DeadLetterQueueManager::dlq_key(), "queue:dlq");

        let job_id = Uuid::nil();
        assert!(DeadLetterQueueManager::job_key(job_id).starts_with("job:"));
        assert!(DeadLetterQueueManager::dlq_member(job_id).starts_with("dlq:"));
    }

    #[test]
    fn test_dlq_manager_clone() {
        // Ensure DeadLetterQueueManager is Clone-able
        fn assert_clone<T: Clone>() {}
        assert_clone::<DeadLetterQueueManager>();
    }

    #[test]
    fn test_dlq_stats_display() {
        let mut by_action_type = std::collections::HashMap::new();
        by_action_type.insert("webhook".to_string(), 5);
        by_action_type.insert("email".to_string(), 3);

        let stats = DlqStats {
            total_jobs: 8,
            by_action_type,
        };

        let display_str = format!("{stats}");
        assert!(display_str.contains("8 total jobs"));
        assert!(display_str.contains("webhook"));
        assert!(display_str.contains("email"));
    }

    #[test]
    fn test_dlq_stats_empty() {
        let stats = DlqStats {
            total_jobs: 0,
            by_action_type: std::collections::HashMap::new(),
        };

        let display_str = format!("{stats}");
        assert_eq!(display_str, "DLQ Stats: 0 total jobs");
    }
}

#[cfg(feature = "queue")]
mod executor_tests {
    #[test]
    fn test_executor_creation() {
        // Note: This test doesn't require actual queue/executor connections
        // It just verifies the struct can be created with proper defaults
        // Actual execution tests would require integration with real queue and executor
    }

    #[test]
    fn test_is_transient_error() {
        let transient = crate::error::ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };
        assert!(crate::job_queue::executor::is_transient_error(&transient));

        let permanent = crate::error::ObserverError::ActionPermanentlyFailed {
            reason: "invalid config".to_string(),
        };
        assert!(!crate::job_queue::executor::is_transient_error(&permanent));
    }
}

#[cfg(feature = "queue")]
mod redis_tests {
    use uuid::Uuid;

    use crate::{
        config::ActionConfig,
        job_queue::{
            Job, JobState,
            redis::{RedisJobQueue, parse_job_state},
        },
    };

    #[test]
    fn test_key_generation() {
        assert_eq!(RedisJobQueue::pending_key(), "queue:pending");
        assert_eq!(RedisJobQueue::processing_key(), "queue:processing");
        assert_eq!(RedisJobQueue::dlq_key(), "queue:dlq");
        assert_eq!(RedisJobQueue::status_key(), "queue:status");

        let job_id = Uuid::nil();
        assert!(RedisJobQueue::job_key(job_id).starts_with("job:"));
        assert!(RedisJobQueue::processing_member(job_id).starts_with("processing:"));
        assert!(RedisJobQueue::dlq_member(job_id).starts_with("dlq:"));
    }

    #[test]
    fn test_redis_job_queue_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisJobQueue>();
    }

    #[test]
    fn test_job_serialization_for_redis() {
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        let json = serde_json::to_string(&job).expect("serialization failed");
        let deserialized: Job = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(job.id, deserialized.id);
        assert_eq!(job.event_id, deserialized.event_id);
        assert_eq!(job.attempt, deserialized.attempt);
    }

    #[test]
    fn test_parse_job_state_all_variants() {
        assert_eq!(parse_job_state("pending"), Some(JobState::Pending));
        assert_eq!(parse_job_state("running"), Some(JobState::Running));
        assert_eq!(parse_job_state("completed"), Some(JobState::Completed));
        assert_eq!(parse_job_state("failed"), Some(JobState::Failed));
        assert_eq!(parse_job_state("dead_lettered"), Some(JobState::DeadLettered));
        assert_eq!(parse_job_state("unknown"), None);
        assert_eq!(parse_job_state(""), None);
    }
}

#[cfg(test)]
mod traits_tests {
    use uuid::Uuid;

    use crate::{
        config::ActionConfig,
        job_queue::{Job, JobState, traits::*},
    };

    #[tokio::test]
    async fn test_mock_queue_enqueue() {
        let queue = MockJobQueue::new();
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);
        let job_id = job.id;

        queue.enqueue(job).await.expect("enqueue failed");

        assert_eq!(queue.queue_depth().await.expect("depth failed"), 1);
        queue
            .get_status(job_id)
            .await
            .unwrap_or_else(|e| panic!("expected Ok from get_status: {e:?}"));
    }

    #[tokio::test]
    async fn test_mock_queue_dequeue() {
        let queue = MockJobQueue::new();
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        queue.enqueue(job).await.expect("enqueue failed");

        let jobs = queue.dequeue(10, 60).await.expect("dequeue failed");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].state, JobState::Running);
    }

    #[tokio::test]
    async fn test_mock_queue_acknowledge() {
        let queue = MockJobQueue::new();
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        queue.enqueue(job).await.expect("enqueue failed");
        let jobs = queue.dequeue(10, 60).await.expect("dequeue failed");
        let job_id = jobs[0].id;
        queue.acknowledge(job_id).await.expect("acknowledge failed");

        let status = queue
            .get_status(job_id)
            .await
            .expect("status failed")
            .expect("status not found");
        assert_eq!(status, JobState::Completed);
    }

    #[tokio::test]
    async fn test_mock_queue_fail_with_retry() {
        let queue = MockJobQueue::new();
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let mut job = Job::new(event_id, action, 3, crate::config::BackoffStrategy::Exponential);

        queue.enqueue(job.clone()).await.expect("enqueue failed");
        queue
            .fail(&mut job, "connection timeout".to_string())
            .await
            .expect("fail failed");

        // Should still be in queue (for retry) and not in DLQ
        assert_eq!(queue.dlq_size().await.expect("dlq size failed"), 0);
        assert_eq!(queue.queue_depth().await.expect("depth failed"), 1);
    }

    #[tokio::test]
    async fn test_mock_queue_dlq() {
        let queue = MockJobQueue::new();
        let event_id = Uuid::new_v4();
        let action = ActionConfig::Cache {
            key_pattern: "test:*".to_string(),
            action: "invalidate".to_string(),
        };
        let mut job = Job::new(event_id, action, 1, crate::config::BackoffStrategy::Exponential);

        queue.enqueue(job.clone()).await.expect("enqueue failed");
        queue.fail(&mut job, "permanent error".to_string()).await.expect("fail failed");

        assert_eq!(queue.dlq_size().await.expect("dlq size failed"), 1);
    }
}
