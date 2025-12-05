"""Tests for GraphQL-specific tracing."""

import pytest

from fraiseql.tracing.graphql_tracing import (
    GraphQLTracer,
    TracingConfig,
)


class TestTracingConfig:
    """Tests for TracingConfig."""

    def test_default_trace_resolvers_true(self):
        config = TracingConfig()
        assert config.trace_resolvers is True

    def test_default_include_variables_false(self):
        """Should not include variables by default (security)."""
        config = TracingConfig()
        assert config.include_variables is False

    def test_default_sanitize_variables_patterns(self):
        """Should have default patterns for sensitive variable names."""
        config = TracingConfig()
        assert "password" in config.sanitize_patterns
        assert "token" in config.sanitize_patterns
        assert "secret" in config.sanitize_patterns

    def test_max_query_length_default(self):
        config = TracingConfig()
        assert config.max_query_length == 1000


class TestVariableSanitization:
    """Tests for variable sanitization in tracing."""

    @pytest.fixture
    def tracer_with_sanitization(self):
        config = TracingConfig(
            include_variables=True,
            sanitize_variables=True,
        )
        return GraphQLTracer(config)

    def test_sanitizes_password_variables(self, tracer_with_sanitization):
        """Should mask password-like variable values."""
        variables = {"username": "alice", "password": "secret123"}
        sanitized = tracer_with_sanitization._sanitize_variables(variables)
        assert sanitized["username"] == "alice"
        assert sanitized["password"] == "[REDACTED]"

    def test_sanitizes_nested_sensitive_fields(self, tracer_with_sanitization):
        """Should mask nested sensitive fields."""
        variables = {
            "input": {
                "email": "alice@example.com",
                "apiToken": "tok_12345",
            }
        }
        sanitized = tracer_with_sanitization._sanitize_variables(variables)
        assert sanitized["input"]["email"] == "alice@example.com"
        assert sanitized["input"]["apiToken"] == "[REDACTED]"

    def test_custom_sanitize_patterns(self):
        """Should support custom sanitization patterns."""
        config = TracingConfig(
            include_variables=True,
            sanitize_variables=True,
            sanitize_patterns=["password", "ssn", "credit_card"],
        )
        tracer = GraphQLTracer(config)
        variables = {"name": "Alice", "ssn": "123-45-6789"}
        sanitized = tracer._sanitize_variables(variables)
        assert sanitized["ssn"] == "[REDACTED]"


class TestGraphQLTracer:
    """Tests for GraphQLTracer."""

    @pytest.fixture
    def tracer(self):
        return GraphQLTracer(TracingConfig())

    def test_detects_query_operation(self, tracer):
        assert tracer._detect_operation_type("query { users }") == "query"
        assert tracer._detect_operation_type("{ users }") == "query"

    def test_detects_mutation_operation(self, tracer):
        assert tracer._detect_operation_type("mutation { createUser }") == "mutation"

    def test_detects_subscription_operation(self, tracer):
        result = tracer._detect_operation_type("subscription { onUserCreated }")
        assert result == "subscription"

    def test_truncates_long_queries(self, tracer):
        long_query = "query { " + "x" * 2000 + " }"
        truncated = tracer._truncate_query(long_query)
        assert len(truncated) <= tracer._config.max_query_length + 3
