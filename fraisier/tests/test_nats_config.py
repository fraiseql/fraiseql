"""Tests for NATS configuration management.

Tests environment variable loading, configuration creation, and integration
with NATS client and event bus initialization.
"""

import os
import pytest
from fraisier.nats.config import (
    NatsConnectionConfig,
    NatsStreamConfig,
    NatsRegionalConfig,
    NatsEventHandlerConfig,
    NatsFullConfig,
    is_nats_enabled,
    get_nats_config,
)


class TestNatsConnectionConfig:
    """Tests for NatsConnectionConfig."""

    def test_default_values(self):
        """Test default configuration values."""
        config = NatsConnectionConfig()

        assert config.servers == ["nats://localhost:4222"]
        assert config.username is None
        assert config.password is None
        assert config.timeout == 5.0
        assert config.max_reconnect_attempts == 60
        assert config.reconnect_time_wait == 2.0

    def test_custom_values(self):
        """Test custom configuration values."""
        config = NatsConnectionConfig(
            servers=["nats://host1:4222", "nats://host2:4222"],
            username="user",
            password="pass",
            timeout=10.0,
            max_reconnect_attempts=120,
            reconnect_time_wait=5.0,
        )

        assert config.servers == ["nats://host1:4222", "nats://host2:4222"]
        assert config.username == "user"
        assert config.password == "pass"
        assert config.timeout == 10.0
        assert config.max_reconnect_attempts == 120
        assert config.reconnect_time_wait == 5.0

    def test_from_env_single_server(self, monkeypatch):
        """Load single server from environment."""
        monkeypatch.setenv("NATS_SERVERS", "nats://my-server:4222")
        monkeypatch.setenv("NATS_USERNAME", "testuser")
        monkeypatch.setenv("NATS_PASSWORD", "testpass")
        monkeypatch.setenv("NATS_TIMEOUT", "10.0")
        monkeypatch.setenv("NATS_MAX_RECONNECT_ATTEMPTS", "100")
        monkeypatch.setenv("NATS_RECONNECT_TIME_WAIT", "3.0")

        config = NatsConnectionConfig.from_env()

        assert config.servers == ["nats://my-server:4222"]
        assert config.username == "testuser"
        assert config.password == "testpass"
        assert config.timeout == 10.0
        assert config.max_reconnect_attempts == 100
        assert config.reconnect_time_wait == 3.0

    def test_from_env_multiple_servers(self, monkeypatch):
        """Load multiple servers from environment (cluster)."""
        monkeypatch.setenv(
            "NATS_SERVERS",
            "nats://server1:4222, nats://server2:4222, nats://server3:4222"
        )

        config = NatsConnectionConfig.from_env()

        assert len(config.servers) == 3
        assert config.servers[0] == "nats://server1:4222"
        assert config.servers[1] == "nats://server2:4222"
        assert config.servers[2] == "nats://server3:4222"

    def test_from_env_no_auth(self, monkeypatch):
        """Load config without authentication."""
        monkeypatch.delenv("NATS_USERNAME", raising=False)
        monkeypatch.delenv("NATS_PASSWORD", raising=False)

        config = NatsConnectionConfig.from_env()

        assert config.username is None
        assert config.password is None

    def test_to_nats_client_kwargs(self):
        """Convert config to NatsClient kwargs."""
        config = NatsConnectionConfig(
            servers=["nats://localhost:4222"],
            username="user",
            password="pass",
            timeout=10.0,
            max_reconnect_attempts=100,
            reconnect_time_wait=3.0,
        )

        kwargs = config.to_nats_client_kwargs()

        assert kwargs["servers"] == ["nats://localhost:4222"]
        assert kwargs["username"] == "user"
        assert kwargs["password"] == "pass"
        assert kwargs["timeout"] == 10.0
        assert kwargs["max_reconnect_attempts"] == 100
        assert kwargs["reconnect_time_wait"] == 3.0

    def test_to_nats_client_kwargs_without_auth(self):
        """Convert config without auth to NatsClient kwargs."""
        config = NatsConnectionConfig()

        kwargs = config.to_nats_client_kwargs()

        assert "username" not in kwargs
        assert "password" not in kwargs
        assert kwargs["servers"] == ["nats://localhost:4222"]


class TestNatsStreamConfig:
    """Tests for NatsStreamConfig."""

    def test_default_values(self):
        """Test default stream configuration."""
        config = NatsStreamConfig()

        assert config.deployment_events_retention_hours == 720
        assert config.health_events_retention_hours == 168
        assert config.database_events_retention_hours == 720
        assert config.metrics_events_retention_hours == 168
        assert config.max_stream_size == 1073741824  # 1GB

    def test_custom_values(self):
        """Test custom stream configuration."""
        config = NatsStreamConfig(
            deployment_events_retention_hours=1000,
            health_events_retention_hours=500,
            database_events_retention_hours=800,
            metrics_events_retention_hours=200,
            max_stream_size=2147483648,  # 2GB
        )

        assert config.deployment_events_retention_hours == 1000
        assert config.health_events_retention_hours == 500
        assert config.database_events_retention_hours == 800
        assert config.metrics_events_retention_hours == 200
        assert config.max_stream_size == 2147483648

    def test_from_env(self, monkeypatch):
        """Load stream config from environment."""
        monkeypatch.setenv("NATS_DEPLOYMENT_EVENTS_RETENTION", "1000")
        monkeypatch.setenv("NATS_HEALTH_EVENTS_RETENTION", "500")
        monkeypatch.setenv("NATS_DATABASE_EVENTS_RETENTION", "800")
        monkeypatch.setenv("NATS_METRICS_EVENTS_RETENTION", "200")
        monkeypatch.setenv("NATS_STREAM_MAX_SIZE", "2147483648")

        config = NatsStreamConfig.from_env()

        assert config.deployment_events_retention_hours == 1000
        assert config.health_events_retention_hours == 500
        assert config.database_events_retention_hours == 800
        assert config.metrics_events_retention_hours == 200
        assert config.max_stream_size == 2147483648

    def test_from_env_defaults(self, monkeypatch):
        """Use defaults when environment variables not set."""
        monkeypatch.delenv("NATS_DEPLOYMENT_EVENTS_RETENTION", raising=False)

        config = NatsStreamConfig.from_env()

        assert config.deployment_events_retention_hours == 720  # default


class TestNatsRegionalConfig:
    """Tests for NatsRegionalConfig."""

    def test_default_values(self):
        """Test default regional configuration."""
        config = NatsRegionalConfig()

        assert config.region == "default"
        assert config.all_regions == ["default"]
        assert config.inter_region_timeout == 30.0

    def test_custom_values(self):
        """Test custom regional configuration."""
        config = NatsRegionalConfig(
            region="us-east-1",
            all_regions=["us-east-1", "us-west-2", "eu-west-1"],
            inter_region_timeout=60.0,
        )

        assert config.region == "us-east-1"
        assert config.all_regions == ["us-east-1", "us-west-2", "eu-west-1"]
        assert config.inter_region_timeout == 60.0

    def test_from_env_single_region(self, monkeypatch):
        """Load single region from environment."""
        monkeypatch.setenv("NATS_REGION", "us-west-2")
        monkeypatch.setenv("DEPLOYMENT_REGIONS", "us-west-2")

        config = NatsRegionalConfig.from_env()

        assert config.region == "us-west-2"
        assert config.all_regions == ["us-west-2"]

    def test_from_env_multiple_regions(self, monkeypatch):
        """Load multiple regions from environment."""
        monkeypatch.setenv("NATS_REGION", "us-east-1")
        monkeypatch.setenv(
            "DEPLOYMENT_REGIONS",
            "us-east-1, us-west-2, eu-west-1"
        )

        config = NatsRegionalConfig.from_env()

        assert config.region == "us-east-1"
        assert len(config.all_regions) == 3
        assert "us-east-1" in config.all_regions
        assert "us-west-2" in config.all_regions
        assert "eu-west-1" in config.all_regions

    def test_from_env_custom_timeout(self, monkeypatch):
        """Load custom inter-region timeout."""
        monkeypatch.setenv("INTER_REGION_TIMEOUT", "60.0")

        config = NatsRegionalConfig.from_env()

        assert config.inter_region_timeout == 60.0


class TestNatsEventHandlerConfig:
    """Tests for NatsEventHandlerConfig."""

    def test_default_values(self):
        """Test default event handler configuration."""
        config = NatsEventHandlerConfig()

        assert config.enable_webhook_notifications is True
        assert config.deployment_webhook_url is None
        assert config.enable_metrics_recording is True
        assert config.enable_event_logging is True

    def test_custom_values(self):
        """Test custom event handler configuration."""
        config = NatsEventHandlerConfig(
            enable_webhook_notifications=False,
            deployment_webhook_url="https://example.com/webhooks",
            enable_metrics_recording=False,
            enable_event_logging=False,
        )

        assert config.enable_webhook_notifications is False
        assert config.deployment_webhook_url == "https://example.com/webhooks"
        assert config.enable_metrics_recording is False
        assert config.enable_event_logging is False

    def test_from_env_all_enabled(self, monkeypatch):
        """Load all features enabled from environment."""
        monkeypatch.setenv("ENABLE_WEBHOOK_NOTIFICATIONS", "true")
        monkeypatch.setenv("DEPLOYMENT_WEBHOOK_URL", "https://example.com/webhooks")
        monkeypatch.setenv("ENABLE_METRICS_RECORDING", "true")
        monkeypatch.setenv("ENABLE_EVENT_LOGGING", "true")

        config = NatsEventHandlerConfig.from_env()

        assert config.enable_webhook_notifications is True
        assert config.deployment_webhook_url == "https://example.com/webhooks"
        assert config.enable_metrics_recording is True
        assert config.enable_event_logging is True

    def test_from_env_all_disabled(self, monkeypatch):
        """Load all features disabled from environment."""
        monkeypatch.setenv("ENABLE_WEBHOOK_NOTIFICATIONS", "false")
        monkeypatch.setenv("ENABLE_METRICS_RECORDING", "false")
        monkeypatch.setenv("ENABLE_EVENT_LOGGING", "false")

        config = NatsEventHandlerConfig.from_env()

        assert config.enable_webhook_notifications is False
        assert config.enable_metrics_recording is False
        assert config.enable_event_logging is False

    def test_from_env_various_bool_formats(self, monkeypatch):
        """Test various boolean string formats."""
        test_cases = [
            ("true", True),
            ("false", False),
            ("1", True),
            ("0", False),
            ("yes", True),
            ("no", False),
            ("on", True),
            ("off", False),
        ]

        for value_str, expected in test_cases:
            monkeypatch.setenv("ENABLE_WEBHOOK_NOTIFICATIONS", value_str)
            config = NatsEventHandlerConfig.from_env()
            assert config.enable_webhook_notifications == expected


class TestNatsFullConfig:
    """Tests for NatsFullConfig."""

    def test_from_env(self, monkeypatch):
        """Load complete NATS configuration from environment."""
        # Connection settings
        monkeypatch.setenv("NATS_SERVERS", "nats://localhost:4222")
        monkeypatch.setenv("NATS_USERNAME", "user")
        monkeypatch.setenv("NATS_PASSWORD", "pass")

        # Stream settings
        monkeypatch.setenv("NATS_DEPLOYMENT_EVENTS_RETENTION", "1000")

        # Regional settings
        monkeypatch.setenv("NATS_REGION", "us-east-1")

        # Handler settings
        monkeypatch.setenv("ENABLE_WEBHOOK_NOTIFICATIONS", "true")

        config = NatsFullConfig.from_env()

        assert config.connection.servers == ["nats://localhost:4222"]
        assert config.connection.username == "user"
        assert config.streams.deployment_events_retention_hours == 1000
        assert config.regional.region == "us-east-1"
        assert config.handlers.enable_webhook_notifications is True

    def test_all_config_sections_present(self):
        """Verify all configuration sections are present."""
        config = NatsFullConfig.from_env()

        assert hasattr(config, "connection")
        assert hasattr(config, "streams")
        assert hasattr(config, "regional")
        assert hasattr(config, "handlers")

        assert isinstance(config.connection, NatsConnectionConfig)
        assert isinstance(config.streams, NatsStreamConfig)
        assert isinstance(config.regional, NatsRegionalConfig)
        assert isinstance(config.handlers, NatsEventHandlerConfig)


class TestNatsConfigHelpers:
    """Tests for NATS configuration helper functions."""

    def test_is_nats_enabled_true(self, monkeypatch):
        """Check if NATS is enabled when NATS_SERVERS is set."""
        monkeypatch.setenv("NATS_SERVERS", "nats://localhost:4222")

        assert is_nats_enabled() is True

    def test_is_nats_enabled_false(self, monkeypatch):
        """Check if NATS is disabled when NATS_SERVERS is not set."""
        monkeypatch.delenv("NATS_SERVERS", raising=False)

        assert is_nats_enabled() is False

    def test_get_nats_config_enabled(self, monkeypatch):
        """Get NATS config when enabled."""
        monkeypatch.setenv("NATS_SERVERS", "nats://localhost:4222")

        config = get_nats_config()

        assert isinstance(config, NatsFullConfig)
        assert config.connection.servers == ["nats://localhost:4222"]

    def test_get_nats_config_disabled_raises_error(self, monkeypatch):
        """Get NATS config raises error when disabled."""
        monkeypatch.delenv("NATS_SERVERS", raising=False)

        with pytest.raises(ValueError, match="NATS is not configured"):
            get_nats_config()


class TestNatsConfigIntegration:
    """Integration tests for NATS configuration."""

    def test_config_to_client_kwargs_flow(self, monkeypatch):
        """Test complete flow from env vars to client kwargs."""
        monkeypatch.setenv("NATS_SERVERS", "nats://my-server:4222")
        monkeypatch.setenv("NATS_USERNAME", "fraisier")
        monkeypatch.setenv("NATS_PASSWORD", "secret")
        monkeypatch.setenv("NATS_TIMEOUT", "10.0")

        config = NatsFullConfig.from_env()
        client_kwargs = config.connection.to_nats_client_kwargs()

        assert client_kwargs["servers"] == ["nats://my-server:4222"]
        assert client_kwargs["username"] == "fraisier"
        assert client_kwargs["password"] == "secret"
        assert client_kwargs["timeout"] == 10.0

    def test_multi_region_config_loading(self, monkeypatch):
        """Test multi-region configuration loading."""
        monkeypatch.setenv("NATS_REGION", "us-east-1")
        monkeypatch.setenv("DEPLOYMENT_REGIONS", "us-east-1,us-west-2,eu-west-1")
        monkeypatch.setenv("INTER_REGION_TIMEOUT", "45.0")

        config = NatsFullConfig.from_env()

        assert config.regional.region == "us-east-1"
        assert len(config.regional.all_regions) == 3
        assert config.regional.inter_region_timeout == 45.0

    def test_retention_settings_vary_by_environment(self, monkeypatch):
        """Test different retention settings for different environments."""
        # Development: shorter retention
        monkeypatch.setenv("NATS_DEPLOYMENT_EVENTS_RETENTION", "168")  # 7 days
        monkeypatch.setenv("NATS_HEALTH_EVENTS_RETENTION", "24")       # 1 day

        config = NatsFullConfig.from_env()

        assert config.streams.deployment_events_retention_hours == 168
        assert config.streams.health_events_retention_hours == 24

    def test_webhook_configuration_when_enabled(self, monkeypatch):
        """Test webhook configuration is complete when enabled."""
        monkeypatch.setenv("ENABLE_WEBHOOK_NOTIFICATIONS", "true")
        monkeypatch.setenv("DEPLOYMENT_WEBHOOK_URL", "https://example.com/webhooks")

        config = NatsFullConfig.from_env()

        assert config.handlers.enable_webhook_notifications is True
        assert config.handlers.deployment_webhook_url == "https://example.com/webhooks"
