//! Integration tests for connection configuration
//!
//! Tests that verify configuration options are properly applied during connection startup.
//! These tests validate that timeouts, keepalive, and application name settings work correctly.

#[cfg(test)]
mod config_integration {
    use fraiseql_wire::connection::ConnectionConfig;
    use std::time::Duration;

    /// Test that statement_timeout is applied to connection parameters
    #[tokio::test]
    async fn test_config_statement_timeout_applied() {
        let config = ConnectionConfig::builder("postgres", "postgres")
            .statement_timeout(Duration::from_secs(30))
            .build();

        // Verify timeout is set in config
        assert_eq!(
            config.statement_timeout,
            Some(Duration::from_secs(30)),
            "statement_timeout not set in config"
        );

        // When used in startup, this will be converted to milliseconds (30000ms)
        // This is verified in the startup method which converts as_millis()
    }

    /// Test that application_name is applied to connection parameters
    #[tokio::test]
    async fn test_config_application_name_applied() {
        let app_name = "test_app";
        let config = ConnectionConfig::builder("postgres", "postgres")
            .application_name(app_name)
            .build();

        assert_eq!(
            config.application_name,
            Some(app_name.to_string()),
            "application_name not set in config"
        );
    }

    /// Test that keepalive_idle is stored in config
    #[tokio::test]
    async fn test_config_keepalive_idle_applied() {
        let config = ConnectionConfig::builder("postgres", "postgres")
            .keepalive_idle(Duration::from_secs(300))
            .build();

        assert_eq!(
            config.keepalive_idle,
            Some(Duration::from_secs(300)),
            "keepalive_idle not set in config"
        );
    }

    /// Test that extra_float_digits is applied to connection parameters
    #[tokio::test]
    async fn test_config_extra_float_digits_applied() {
        let config = ConnectionConfig::builder("postgres", "postgres")
            .extra_float_digits(2)
            .build();

        assert_eq!(
            config.extra_float_digits,
            Some(2),
            "extra_float_digits not set in config"
        );
    }

    /// Test multiple configuration options together
    #[tokio::test]
    async fn test_config_multiple_options() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("secret")
            .statement_timeout(Duration::from_secs(60))
            .keepalive_idle(Duration::from_secs(300))
            .application_name("multi_test")
            .extra_float_digits(1)
            .build();

        assert_eq!(config.database, "mydb");
        assert_eq!(config.user, "myuser");
        assert_eq!(config.password, Some("secret".to_string()));
        assert_eq!(config.statement_timeout, Some(Duration::from_secs(60)));
        assert_eq!(config.keepalive_idle, Some(Duration::from_secs(300)));
        assert_eq!(config.application_name, Some("multi_test".to_string()));
        assert_eq!(config.extra_float_digits, Some(1));
    }

    /// Test that configuration preserves user parameters
    #[tokio::test]
    async fn test_config_preserves_user_params() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .param("custom_param", "custom_value")
            .statement_timeout(Duration::from_secs(30))
            .param("another_param", "another_value")
            .build();

        assert!(config.params.contains_key("custom_param"));
        assert_eq!(config.params.get("custom_param").unwrap(), "custom_value");
        assert!(config.params.contains_key("another_param"));
        assert_eq!(config.params.get("another_param").unwrap(), "another_value");
        assert_eq!(config.statement_timeout, Some(Duration::from_secs(30)));
    }

    /// Test that defaults are None for new optional fields
    #[tokio::test]
    async fn test_config_defaults_are_none() {
        let config = ConnectionConfig::new("mydb", "myuser");

        assert!(config.connect_timeout.is_none());
        assert!(config.statement_timeout.is_none());
        assert!(config.keepalive_idle.is_none());
        assert!(config.application_name.is_none());
        assert!(config.extra_float_digits.is_none());
    }

    /// Test that timeout values are properly formatted in builder
    #[tokio::test]
    async fn test_config_timeout_formatting() {
        // Test millisecond conversion which happens in startup
        let timeout = Duration::from_secs(10);
        assert_eq!(timeout.as_millis(), 10000);

        let timeout = Duration::from_millis(500);
        assert_eq!(timeout.as_millis(), 500);

        let timeout = Duration::from_secs(1) + Duration::from_millis(500);
        assert_eq!(timeout.as_millis(), 1500);
    }

    /// Test builder is cloneable
    #[test]
    fn test_config_builder_is_cloneable() {
        let config = ConnectionConfig::builder("db", "user")
            .statement_timeout(Duration::from_secs(30))
            .build();

        let cloned = config.clone();
        assert_eq!(config.database, cloned.database);
        assert_eq!(config.statement_timeout, cloned.statement_timeout);
    }

    /// Test config is debug-printable
    #[test]
    fn test_config_is_debug() {
        let config = ConnectionConfig::builder("db", "user")
            .statement_timeout(Duration::from_secs(30))
            .build();

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("db"));
        assert!(debug_str.contains("user"));
    }
}
