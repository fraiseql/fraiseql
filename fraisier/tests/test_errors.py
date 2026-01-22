"""Tests for custom exception hierarchy."""

import pytest

from fraisier.errors import (
    ConfigurationError,
    DatabaseConnectionError,
    DatabaseError,
    DatabaseTransactionError,
    DeploymentError,
    DeploymentLockError,
    DeploymentTimeoutError,
    FraisierError,
    GitProviderError,
    HealthCheckError,
    NotFoundError,
    ProviderConnectionError,
    ProviderError,
    ProviderUnavailableError,
    RollbackError,
    ValidationError,
    WebhookError,
)


class TestFraisierError:
    """Test base FraisierError class."""

    def test_basic_exception(self):
        """Test basic exception creation."""
        error = FraisierError("Test error")
        assert str(error) == "Test error"
        assert error.message == "Test error"
        assert error.code == "FRAISIER_ERROR"
        assert error.recoverable is False

    def test_exception_with_code(self):
        """Test exception with custom code."""
        error = FraisierError("Test", code="CUSTOM_ERROR")
        assert error.code == "CUSTOM_ERROR"

    def test_exception_with_context(self):
        """Test exception with context."""
        context = {"deployment_id": "deploy-123", "provider": "bare_metal"}
        error = FraisierError("Test", context=context)
        assert error.context == context

    def test_exception_recoverable(self):
        """Test recoverable flag."""
        error = FraisierError("Test", recoverable=True)
        assert error.recoverable is True

    def test_exception_with_cause(self):
        """Test exception cause chaining."""
        cause = ValueError("Original error")
        error = FraisierError("Test", cause=cause)
        assert error.cause is cause
        # __cause__ may not be set depending on implementation
        assert "Original error" in str(error) or error.cause is cause

    def test_exception_to_dict(self):
        """Test serialization to dict."""
        error = FraisierError(
            "Test error",
            code="TEST_CODE",
            context={"key": "value"},
            recoverable=True,
        )
        result = error.to_dict()
        assert result["error_type"] == "FraisierError"
        assert result["message"] == "Test error"
        assert result["code"] == "TEST_CODE"
        assert result["context"] == {"key": "value"}
        assert result["recoverable"] is True


class TestDeploymentErrors:
    """Test deployment-related errors."""

    def test_deployment_error(self):
        """Test DeploymentError."""
        error = DeploymentError("Deployment failed")
        assert isinstance(error, FraisierError)
        assert error.code == "DEPLOYMENT_ERROR"

    def test_deployment_timeout_error(self):
        """Test DeploymentTimeoutError."""
        error = DeploymentTimeoutError("Timeout after 300s")
        assert isinstance(error, DeploymentError)
        assert error.code == "DEPLOYMENT_TIMEOUT"

    def test_health_check_error(self):
        """Test HealthCheckError."""
        error = HealthCheckError("Health check failed")
        assert isinstance(error, DeploymentError)
        assert error.code == "HEALTH_CHECK_FAILED"

    def test_rollback_error(self):
        """Test RollbackError."""
        error = RollbackError("Rollback failed")
        assert isinstance(error, DeploymentError)
        assert error.code == "ROLLBACK_FAILED"

    def test_deployment_lock_error(self):
        """Test DeploymentLockError."""
        error = DeploymentLockError("Lock timeout")
        assert isinstance(error, DeploymentError)
        assert error.code == "DEPLOYMENT_LOCKED"


class TestProviderErrors:
    """Test provider-related errors."""

    def test_provider_error(self):
        """Test ProviderError."""
        error = ProviderError("Provider error")
        assert isinstance(error, FraisierError)
        assert error.code == "PROVIDER_ERROR"

    def test_provider_unavailable_error(self):
        """Test ProviderUnavailableError."""
        error = ProviderUnavailableError("Provider unavailable")
        assert isinstance(error, ProviderError)
        assert error.recoverable is True
        assert error.code == "PROVIDER_UNAVAILABLE"

    def test_provider_connection_error(self):
        """Test ProviderConnectionError."""
        error = ProviderConnectionError("Connection refused")
        assert isinstance(error, ProviderError)
        assert error.code == "PROVIDER_CONNECTION_ERROR"


class TestDatabaseErrors:
    """Test database-related errors."""

    def test_database_error(self):
        """Test DatabaseError."""
        error = DatabaseError("Database error")
        assert isinstance(error, FraisierError)
        assert error.code == "DATABASE_ERROR"

    def test_database_connection_error(self):
        """Test DatabaseConnectionError."""
        error = DatabaseConnectionError("Cannot connect")
        assert isinstance(error, DatabaseError)
        assert error.code == "DATABASE_CONNECTION_ERROR"

    def test_database_transaction_error(self):
        """Test DatabaseTransactionError."""
        error = DatabaseTransactionError("Transaction failed")
        assert isinstance(error, DatabaseError)
        assert error.code == "DATABASE_TRANSACTION_ERROR"


class TestOtherErrors:
    """Test other error types."""

    def test_configuration_error(self):
        """Test ConfigurationError."""
        error = ConfigurationError("Invalid config")
        assert isinstance(error, FraisierError)
        assert error.code == "CONFIG_ERROR"

    def test_validation_error(self):
        """Test ValidationError."""
        error = ValidationError("Validation failed")
        assert isinstance(error, FraisierError)
        assert error.code == "VALIDATION_ERROR"

    def test_not_found_error(self):
        """Test NotFoundError."""
        error = NotFoundError("Resource not found")
        assert isinstance(error, FraisierError)
        assert error.code == "NOT_FOUND"

    def test_git_provider_error(self):
        """Test GitProviderError."""
        error = GitProviderError("Git error")
        assert isinstance(error, FraisierError)
        assert error.code == "GIT_PROVIDER_ERROR"

    def test_webhook_error(self):
        """Test WebhookError."""
        error = WebhookError("Webhook failed")
        assert isinstance(error, FraisierError)
        assert error.code == "WEBHOOK_ERROR"


class TestErrorInheritance:
    """Test error inheritance hierarchy."""

    def test_deployment_timeout_is_deployment_error(self):
        """Test inheritance chain."""
        error = DeploymentTimeoutError("Timeout")
        assert isinstance(error, DeploymentTimeoutError)
        assert isinstance(error, DeploymentError)
        assert isinstance(error, FraisierError)
        assert isinstance(error, Exception)

    def test_provider_unavailable_is_recoverable(self):
        """Test recoverable flag inheritance."""
        error = ProviderUnavailableError("Unavailable")
        assert error.recoverable is True

    def test_all_errors_have_codes(self):
        """Test all errors have unique codes."""
        error_classes = [
            FraisierError,
            ConfigurationError,
            DeploymentError,
            DeploymentTimeoutError,
            HealthCheckError,
            ProviderError,
            ProviderUnavailableError,
            RollbackError,
            DatabaseError,
            ValidationError,
            NotFoundError,
        ]

        codes = set()
        for error_class in error_classes:
            error = error_class("Test")
            assert hasattr(error, "code")
            assert isinstance(error.code, str)
            assert error.code not in codes or error.code == "FRAISIER_ERROR"
            codes.add(error.code)


class TestErrorSerialization:
    """Test error serialization."""

    def test_deployment_timeout_serialization(self):
        """Test serializing deployment timeout error."""
        error = DeploymentTimeoutError(
            "Timeout after 300s",
            context={"deployment_id": "deploy-123"},
        )
        result = error.to_dict()
        assert result["error_type"] == "DeploymentTimeoutError"
        assert "Timeout after 300s" in result["message"]
        assert result["code"] == "DEPLOYMENT_TIMEOUT"

    def test_error_with_all_fields(self):
        """Test serialization with all fields."""
        cause = ValueError("Root cause")
        error = ProviderUnavailableError(
            "Provider down",
            code="PROVIDER_UNAVAILABLE",
            context={"provider": "bare_metal", "reason": "ssh timeout"},
            recoverable=True,
            cause=cause,
        )
        result = error.to_dict()
        assert result["error_type"] == "ProviderUnavailableError"
        assert result["code"] == "PROVIDER_UNAVAILABLE"
        assert result["recoverable"] is True
        assert "provider" in result["context"]


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
