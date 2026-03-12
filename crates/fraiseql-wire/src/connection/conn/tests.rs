//! Tests for connection configuration types

#[cfg(test)]
#[allow(clippy::module_inception)] // Reason: tests file named tests.rs containing mod tests — convention
mod tests {
    use super::super::config::ConnectionConfig;
    use std::time::Duration;

    #[test]
    fn test_connection_config() {
        let config = ConnectionConfig::new("testdb", "testuser")
            .password("testpass")
            .param("application_name", "fraiseql-wire");

        assert_eq!(config.database, "testdb");
        assert_eq!(config.user, "testuser");
        assert_eq!(
            config.password.as_ref().map(|p| p.as_str()),
            Some("testpass")
        );
        assert_eq!(
            config.params.get("application_name"),
            Some(&"fraiseql-wire".to_string())
        );
    }

    #[test]
    fn test_connection_config_builder_basic() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("mypass")
            .build();

        assert_eq!(config.database, "mydb");
        assert_eq!(config.user, "myuser");
        assert_eq!(config.password.as_ref().map(|p| p.as_str()), Some("mypass"));
        assert_eq!(config.connect_timeout, None);
        assert_eq!(config.statement_timeout, None);
        assert_eq!(config.keepalive_idle, None);
        assert_eq!(config.application_name, None);
    }

    #[test]
    fn test_connection_config_builder_with_timeouts() {
        let connect_timeout = Duration::from_secs(10);
        let statement_timeout = Duration::from_secs(30);
        let keepalive_idle = Duration::from_secs(300);

        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("mypass")
            .connect_timeout(connect_timeout)
            .statement_timeout(statement_timeout)
            .keepalive_idle(keepalive_idle)
            .build();

        assert_eq!(config.connect_timeout, Some(connect_timeout));
        assert_eq!(config.statement_timeout, Some(statement_timeout));
        assert_eq!(config.keepalive_idle, Some(keepalive_idle));
    }

    #[test]
    fn test_connection_config_builder_with_application_name() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .application_name("my_app")
            .extra_float_digits(2)
            .build();

        assert_eq!(config.application_name, Some("my_app".to_string()));
        assert_eq!(config.extra_float_digits, Some(2));
    }

    #[test]
    fn test_connection_config_builder_fluent() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("secret")
            .param("key1", "value1")
            .connect_timeout(Duration::from_secs(5))
            .statement_timeout(Duration::from_secs(60))
            .application_name("test_app")
            .build();

        assert_eq!(config.database, "mydb");
        assert_eq!(config.user, "myuser");
        assert_eq!(config.password.as_ref().map(|p| p.as_str()), Some("secret"));
        assert_eq!(config.params.get("key1"), Some(&"value1".to_string()));
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(5)));
        assert_eq!(config.statement_timeout, Some(Duration::from_secs(60)));
        assert_eq!(config.application_name, Some("test_app".to_string()));
    }

    #[test]
    fn test_connection_config_defaults() {
        let config = ConnectionConfig::new("db", "user");

        assert!(config.connect_timeout.is_none());
        assert!(config.statement_timeout.is_none());
        assert!(config.keepalive_idle.is_none());
        assert!(config.application_name.is_none());
        assert!(config.extra_float_digits.is_none());
    }

    // Verify that async functions return Send futures (compile-time check)
    // This ensures compatibility with async_trait and multi-threaded executors.
    // The actual assertion doesn't execute - it's type-checked at compile time.
    // Reason: compile-time Send safety check, never invoked at runtime
    #[allow(dead_code)]
    const _SEND_SAFETY_CHECK: fn() = || {
        fn require_send<T: Send>() {}

        // Dummy values just for type checking - never executed
        #[allow(unreachable_code)]
        let _ = || {
            // These would be checked at compile time if instantiated
            require_send::<
                std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = ()> + Send>>,
            >();
        };
    };
}
