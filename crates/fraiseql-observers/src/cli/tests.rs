//! CLI command tests

#[cfg(test)]
mod command_tests {
    use std::str::FromStr;

    use crate::cli::OutputFormat;

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(OutputFormat::from_str("text").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str("TEXT").unwrap(), OutputFormat::Text);
    }

    #[test]
    fn test_output_format_invalid() {
        assert!(OutputFormat::from_str("invalid").is_err());
        assert!(OutputFormat::from_str("yaml").is_err());
    }

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Text.to_string(), "text");
        assert_eq!(OutputFormat::Json.to_string(), "json");
    }

    #[tokio::test]
    async fn test_status_command_text_format() {
        let result = crate::cli::commands::status::execute(OutputFormat::Text, None, false).await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_status_command_json_format() {
        let result = crate::cli::commands::status::execute(OutputFormat::Json, None, false).await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_debug_event_command() {
        let result =
            crate::cli::commands::debug_event::execute(OutputFormat::Text, None, None, None, None)
                .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_debug_event_with_filters() {
        let result = crate::cli::commands::debug_event::execute(
            OutputFormat::Json,
            Some("evt-123".to_string()),
            Some(10),
            Some("Order".to_string()),
            Some("created".to_string()),
        )
        .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_validate_config_command() {
        let result =
            crate::cli::commands::validate_config::execute(OutputFormat::Text, None, false).await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_validate_config_detailed() {
        let result =
            crate::cli::commands::validate_config::execute(OutputFormat::Json, None, true).await;
        result.expect("CLI command should succeed");
    }

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn test_metrics_command() {
        let result = crate::cli::commands::metrics::execute(OutputFormat::Text, None, false).await;
        result.expect("CLI command should succeed");
    }

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn test_metrics_json_format() {
        let result = crate::cli::commands::metrics::execute(OutputFormat::Json, None, false).await;
        result.expect("CLI command should succeed");
    }
}

#[cfg(test)]
mod dlq_command_tests {
    use crate::cli::{DlqSubcommand, OutputFormat, commands::dlq};

    #[tokio::test]
    async fn test_dlq_list_command() {
        let result = dlq::execute(
            OutputFormat::Text,
            DlqSubcommand::List {
                limit:    10,
                offset:   None,
                observer: None,
                after:    None,
            },
        )
        .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_dlq_show_command() {
        let result = dlq::execute(
            OutputFormat::Json,
            DlqSubcommand::Show {
                item_id: "dlq-001".to_string(),
            },
        )
        .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_dlq_retry_command() {
        let result = dlq::execute(
            OutputFormat::Text,
            DlqSubcommand::Retry {
                item_id: "dlq-001".to_string(),
                force:   false,
            },
        )
        .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_dlq_retry_all_command() {
        let result = dlq::execute(
            OutputFormat::Json,
            DlqSubcommand::RetryAll {
                observer: None,
                after:    None,
                dry_run:  true,
            },
        )
        .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_dlq_remove_command() {
        let result = dlq::execute(
            OutputFormat::Text,
            DlqSubcommand::Remove {
                item_id: "dlq-001".to_string(),
                force:   false,
            },
        )
        .await;
        result.expect("CLI command should succeed");
    }

    #[tokio::test]
    async fn test_dlq_stats_command() {
        let result = dlq::execute(
            OutputFormat::Json,
            DlqSubcommand::Stats {
                by_observer: true,
                by_error:    true,
            },
        )
        .await;
        result.expect("CLI command should succeed");
    }
}
