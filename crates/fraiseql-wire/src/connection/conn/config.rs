//! Connection configuration types

use std::collections::HashMap;
use std::time::Duration;
use zeroize::Zeroizing;

/// Connection configuration
///
/// Stores connection parameters including database, credentials, and optional timeouts.
/// Use `ConnectionConfig::builder()` for advanced configuration with timeouts and keepalive.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Database name
    pub database: String,
    /// Username
    pub user: String,
    /// Password (optional, zeroed on drop for security)
    pub password: Option<Zeroizing<String>>,
    /// Additional connection parameters
    pub params: HashMap<String, String>,
    /// TCP connection timeout (default: 10 seconds)
    pub connect_timeout: Option<Duration>,
    /// Query statement timeout
    pub statement_timeout: Option<Duration>,
    /// TCP keepalive idle interval (default: 5 minutes)
    pub keepalive_idle: Option<Duration>,
    /// Application name for Postgres logs (default: "fraiseql-wire")
    pub application_name: Option<String>,
    /// Postgres `extra_float_digits` setting
    pub extra_float_digits: Option<i32>,
}

impl ConnectionConfig {
    /// Create new configuration with defaults
    ///
    /// # Arguments
    ///
    /// * `database` - Database name
    /// * `user` - Username
    ///
    /// # Defaults
    ///
    /// - `connect_timeout`: None
    /// - `statement_timeout`: None
    /// - `keepalive_idle`: None
    /// - `application_name`: None
    /// - `extra_float_digits`: None
    ///
    /// For configured timeouts and keepalive, use `builder()` instead.
    pub fn new(database: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            user: user.into(),
            password: None,
            params: HashMap::new(),
            connect_timeout: None,
            statement_timeout: None,
            keepalive_idle: None,
            application_name: None,
            extra_float_digits: None,
        }
    }

    /// Create a builder for advanced configuration
    ///
    /// Use this to configure timeouts, keepalive, and application name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fraiseql_wire::connection::ConnectionConfig;
    /// use std::time::Duration;
    /// let config = ConnectionConfig::builder("mydb", "user")
    ///     .connect_timeout(Duration::from_secs(10))
    ///     .statement_timeout(Duration::from_secs(30))
    ///     .build();
    /// ```
    pub fn builder(
        database: impl Into<String>,
        user: impl Into<String>,
    ) -> ConnectionConfigBuilder {
        ConnectionConfigBuilder {
            database: database.into(),
            user: user.into(),
            password: None,
            params: HashMap::new(),
            connect_timeout: None,
            statement_timeout: None,
            keepalive_idle: None,
            application_name: None,
            extra_float_digits: None,
        }
    }

    /// Set password (automatically zeroed on drop)
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(Zeroizing::new(password.into()));
        self
    }

    /// Add connection parameter
    pub fn param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// Builder for creating `ConnectionConfig` with advanced options
///
/// Provides a fluent API for configuring timeouts, keepalive, and application name.
///
/// # Examples
///
/// ```rust
/// use fraiseql_wire::connection::ConnectionConfig;
/// use std::time::Duration;
/// let config = ConnectionConfig::builder("mydb", "user")
///     .password("secret")
///     .connect_timeout(Duration::from_secs(10))
///     .statement_timeout(Duration::from_secs(30))
///     .keepalive_idle(Duration::from_secs(300))
///     .application_name("my_app")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ConnectionConfigBuilder {
    pub(super) database: String,
    pub(super) user: String,
    pub(super) password: Option<Zeroizing<String>>,
    pub(super) params: HashMap<String, String>,
    pub(super) connect_timeout: Option<Duration>,
    pub(super) statement_timeout: Option<Duration>,
    pub(super) keepalive_idle: Option<Duration>,
    pub(super) application_name: Option<String>,
    pub(super) extra_float_digits: Option<i32>,
}

impl ConnectionConfigBuilder {
    /// Set the password (automatically zeroed on drop)
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(Zeroizing::new(password.into()));
        self
    }

    /// Add a connection parameter
    pub fn param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Set TCP connection timeout
    ///
    /// Default: None (no timeout)
    ///
    /// # Arguments
    ///
    /// * `duration` - Timeout duration for establishing TCP connection
    pub const fn connect_timeout(mut self, duration: Duration) -> Self {
        self.connect_timeout = Some(duration);
        self
    }

    /// Set statement (query) timeout
    ///
    /// Default: None (unlimited)
    ///
    /// # Arguments
    ///
    /// * `duration` - Timeout duration for query execution
    pub const fn statement_timeout(mut self, duration: Duration) -> Self {
        self.statement_timeout = Some(duration);
        self
    }

    /// Set TCP keepalive idle interval
    ///
    /// Default: None (OS default)
    ///
    /// # Arguments
    ///
    /// * `duration` - Idle duration before sending keepalive probes
    pub const fn keepalive_idle(mut self, duration: Duration) -> Self {
        self.keepalive_idle = Some(duration);
        self
    }

    /// Set application name for Postgres logs
    ///
    /// Default: None (Postgres will not set `application_name`)
    ///
    /// # Arguments
    ///
    /// * `name` - Application name to identify in Postgres logs
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = Some(name.into());
        self
    }

    /// Set `extra_float_digits` for float precision
    ///
    /// Default: None (use Postgres default)
    ///
    /// # Arguments
    ///
    /// * `digits` - Number of extra digits (typically 0-2)
    pub const fn extra_float_digits(mut self, digits: i32) -> Self {
        self.extra_float_digits = Some(digits);
        self
    }

    /// Build the configuration
    pub fn build(self) -> ConnectionConfig {
        ConnectionConfig {
            database: self.database,
            user: self.user,
            password: self.password,
            params: self.params,
            connect_timeout: self.connect_timeout,
            statement_timeout: self.statement_timeout,
            keepalive_idle: self.keepalive_idle,
            application_name: self.application_name,
            extra_float_digits: self.extra_float_digits,
        }
    }
}
