"""Unit tests for observability configuration (Phase 19, Commit 1).

Tests for observability settings in FraiseQLConfig.
"""

import os
import pytest
from fraiseql.fastapi.config import FraiseQLConfig


class TestObservabilityConfiguration:
    """Tests for observability configuration fields."""

    def test_observability_defaults(self) -> None:
        """Test default observability configuration values."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        # All observability features enabled by default
        assert config.observability_enabled is True
        assert config.metrics_enabled is True
        assert config.tracing_enabled is True

    def test_observability_can_be_disabled(self) -> None:
        """Test disabling observability features."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            observability_enabled=False,
            metrics_enabled=False,
            tracing_enabled=False,
        )

        assert config.observability_enabled is False
        assert config.metrics_enabled is False
        assert config.tracing_enabled is False

    def test_trace_sample_rate_validation(self) -> None:
        """Test trace sample rate is validated (0.0-1.0)."""
        # Valid rates
        config1 = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            trace_sample_rate=0.0,
        )
        assert config1.trace_sample_rate == 0.0

        config2 = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            trace_sample_rate=0.5,
        )
        assert config2.trace_sample_rate == 0.5

        config3 = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            trace_sample_rate=1.0,
        )
        assert config3.trace_sample_rate == 1.0

    def test_trace_sample_rate_invalid(self) -> None:
        """Test trace sample rate rejects invalid values."""
        with pytest.raises(ValueError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                trace_sample_rate=-0.1,
            )

        with pytest.raises(ValueError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                trace_sample_rate=1.1,
            )

    def test_slow_query_threshold_defaults(self) -> None:
        """Test slow query threshold default value."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        assert config.slow_query_threshold_ms == 100

    def test_slow_query_threshold_can_be_set(self) -> None:
        """Test slow query threshold can be configured."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            slow_query_threshold_ms=500,
        )
        assert config.slow_query_threshold_ms == 500

    def test_slow_query_threshold_must_be_positive(self) -> None:
        """Test slow query threshold must be > 0."""
        with pytest.raises(ValueError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                slow_query_threshold_ms=0,
            )

        with pytest.raises(ValueError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                slow_query_threshold_ms=-100,
            )

    def test_privacy_settings(self) -> None:
        """Test privacy-related settings."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        # Privacy settings disabled by default (secure by default)
        assert config.include_query_bodies is False
        assert config.include_variable_values is False

    def test_privacy_settings_can_be_enabled(self) -> None:
        """Test privacy settings can be enabled (for development)."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            include_query_bodies=True,
            include_variable_values=True,
        )

        assert config.include_query_bodies is True
        assert config.include_variable_values is True

    def test_audit_log_retention_defaults(self) -> None:
        """Test audit log retention default."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        assert config.audit_log_retention_days == 90

    def test_audit_log_retention_can_be_set(self) -> None:
        """Test audit log retention can be configured."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            audit_log_retention_days=30,
        )
        assert config.audit_log_retention_days == 30

    def test_audit_log_retention_must_be_positive(self) -> None:
        """Test audit log retention must be > 0."""
        with pytest.raises(ValueError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                audit_log_retention_days=0,
            )

    def test_health_check_timeout_defaults(self) -> None:
        """Test health check timeout default."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        assert config.health_check_timeout_ms == 5000

    def test_health_check_timeout_can_be_set(self) -> None:
        """Test health check timeout can be configured."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            health_check_timeout_ms=10000,
        )
        assert config.health_check_timeout_ms == 10000

    def test_health_check_timeout_must_be_positive(self) -> None:
        """Test health check timeout must be > 0."""
        with pytest.raises(ValueError):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                health_check_timeout_ms=0,
            )


class TestObservabilityEnvironmentVariables:
    """Tests for loading observability config from environment variables."""

    def test_observability_from_env(self, monkeypatch) -> None:
        """Test loading observability settings from environment."""
        monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
        monkeypatch.setenv("FRAISEQL_OBSERVABILITY_ENABLED", "false")
        monkeypatch.setenv("FRAISEQL_METRICS_ENABLED", "false")
        monkeypatch.setenv("FRAISEQL_TRACING_ENABLED", "false")

        config = FraiseQLConfig()
        assert config.observability_enabled is False
        assert config.metrics_enabled is False
        assert config.tracing_enabled is False

    def test_trace_sample_rate_from_env(self, monkeypatch) -> None:
        """Test loading trace sample rate from environment."""
        monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
        monkeypatch.setenv("FRAISEQL_TRACE_SAMPLE_RATE", "0.5")

        config = FraiseQLConfig()
        assert config.trace_sample_rate == 0.5

    def test_slow_query_threshold_from_env(self, monkeypatch) -> None:
        """Test loading slow query threshold from environment."""
        monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
        monkeypatch.setenv("FRAISEQL_SLOW_QUERY_THRESHOLD_MS", "250")

        config = FraiseQLConfig()
        assert config.slow_query_threshold_ms == 250

    def test_audit_retention_from_env(self, monkeypatch) -> None:
        """Test loading audit retention from environment."""
        monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
        monkeypatch.setenv("FRAISEQL_AUDIT_LOG_RETENTION_DAYS", "180")

        config = FraiseQLConfig()
        assert config.audit_log_retention_days == 180

    def test_health_check_timeout_from_env(self, monkeypatch) -> None:
        """Test loading health check timeout from environment."""
        monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
        monkeypatch.setenv("FRAISEQL_HEALTH_CHECK_TIMEOUT_MS", "3000")

        config = FraiseQLConfig()
        assert config.health_check_timeout_ms == 3000


class TestObservabilityIntegration:
    """Integration tests for observability configuration."""

    def test_all_observability_settings_together(self) -> None:
        """Test all observability settings can be used together."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="production",
            observability_enabled=True,
            metrics_enabled=True,
            tracing_enabled=True,
            trace_sample_rate=0.1,  # 10% sampling in production
            slow_query_threshold_ms=200,
            include_query_bodies=False,  # No PII
            include_variable_values=False,
            audit_log_retention_days=365,
            health_check_timeout_ms=5000,
        )

        assert config.observability_enabled is True
        assert config.metrics_enabled is True
        assert config.tracing_enabled is True
        assert config.trace_sample_rate == 0.1
        assert config.slow_query_threshold_ms == 200
        assert config.include_query_bodies is False
        assert config.include_variable_values is False
        assert config.audit_log_retention_days == 365
        assert config.health_check_timeout_ms == 5000

    def test_development_observability_settings(self) -> None:
        """Test typical development observability settings."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="development",
            trace_sample_rate=1.0,  # Trace all in dev
            include_query_bodies=True,  # Debug info
            slow_query_threshold_ms=50,  # Catch even minor slow queries
        )

        assert config.trace_sample_rate == 1.0
        assert config.include_query_bodies is True
        assert config.slow_query_threshold_ms == 50

    def test_production_observability_settings(self) -> None:
        """Test typical production observability settings."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="production",
            trace_sample_rate=0.1,  # Sample 10%
            include_query_bodies=False,  # Privacy
            include_variable_values=False,  # Privacy
            slow_query_threshold_ms=500,  # Only alert on significant slowness
        )

        assert config.trace_sample_rate == 0.1
        assert config.include_query_bodies is False
        assert config.include_variable_values is False
        assert config.slow_query_threshold_ms == 500
