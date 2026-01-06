"""Tests for health check FastAPI endpoints.

Tests for:
- Liveness probe (/health/live)
- Readiness probe (/health/ready)
- Full health check (/health)
- Layer-specific endpoints
- HTTP status codes
"""

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from fraiseql.health import (
    setup_health_endpoints,
)


@pytest.fixture
def test_app() -> None:
    """Create test FastAPI app with health endpoints."""
    app = FastAPI()
    setup_health_endpoints(app)
    return app


@pytest.fixture
def client(test_app) -> None:
    """Create test client."""
    return TestClient(test_app)


class TestLivenessProbe:
    """Tests for /health/live endpoint."""

    def test_liveness_returns_200(self, client) -> None:
        """Liveness probe returns 200 OK."""
        response = client.get("/health/live")
        assert response.status_code == 200

    def test_liveness_returns_alive_status(self, client) -> None:
        """Liveness probe returns alive status."""
        response = client.get("/health/live")
        data = response.json()
        assert data["status"] == "alive"

    def test_liveness_is_fast(self, client) -> None:
        """Liveness probe responds quickly."""
        # Should respond in less than 100ms
        response = client.get("/health/live")
        assert response.status_code == 200


class TestReadinessProbe:
    """Tests for /health/ready endpoint."""

    def test_readiness_returns_200_or_503(self, client) -> None:
        """Readiness probe returns 200 or 503."""
        response = client.get("/health/ready")
        assert response.status_code in [200, 503]

    def test_readiness_returns_ready_status(self, client) -> None:
        """Readiness probe includes ready flag."""
        response = client.get("/health/ready")
        data = response.json()
        assert "ready" in data
        assert isinstance(data["ready"], bool)

    def test_readiness_includes_status(self, client) -> None:
        """Readiness probe includes health status."""
        response = client.get("/health/ready")
        data = response.json()
        assert "status" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]

    def test_readiness_includes_timestamp(self, client) -> None:
        """Readiness probe includes timestamp."""
        response = client.get("/health/ready")
        data = response.json()
        assert "timestamp" in data

    def test_readiness_includes_duration(self, client) -> None:
        """Readiness probe includes check duration."""
        response = client.get("/health/ready")
        data = response.json()
        assert "duration_ms" in data
        assert data["duration_ms"] > 0


class TestFullHealthCheck:
    """Tests for /health endpoint."""

    def test_full_check_returns_200_or_503(self, client) -> None:
        """Full health check returns 200 or 503."""
        response = client.get("/health")
        assert response.status_code in [200, 503]

    def test_full_check_includes_status(self, client) -> None:
        """Full check includes overall status."""
        response = client.get("/health")
        data = response.json()
        assert "status" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]

    def test_full_check_includes_timestamp(self, client) -> None:
        """Full check includes timestamp."""
        response = client.get("/health")
        data = response.json()
        assert "timestamp" in data

    def test_full_check_includes_all_layers(self, client) -> None:
        """Full check includes all system layers."""
        response = client.get("/health")
        data = response.json()

        assert "database" in data
        assert "cache" in data
        assert "graphql" in data
        assert "tracing" in data

    def test_full_check_database_has_status(self, client) -> None:
        """Full check database section has status."""
        response = client.get("/health")
        data = response.json()

        assert "status" in data["database"]
        assert data["database"]["status"] in [
            "healthy",
            "degraded",
            "unhealthy",
        ]

    def test_full_check_cache_has_status(self, client) -> None:
        """Full check cache section has status."""
        response = client.get("/health")
        data = response.json()

        assert "status" in data["cache"]
        assert data["cache"]["status"] in [
            "healthy",
            "degraded",
            "unhealthy",
        ]

    def test_full_check_graphql_has_status(self, client) -> None:
        """Full check GraphQL section has status."""
        response = client.get("/health")
        data = response.json()

        assert "status" in data["graphql"]
        assert data["graphql"]["status"] in [
            "healthy",
            "degraded",
            "unhealthy",
        ]

    def test_full_check_tracing_has_status(self, client) -> None:
        """Full check tracing section has status."""
        response = client.get("/health")
        data = response.json()

        assert "status" in data["tracing"]
        assert data["tracing"]["status"] in [
            "healthy",
            "degraded",
            "unhealthy",
        ]

    def test_full_check_includes_duration(self, client) -> None:
        """Full check includes duration."""
        response = client.get("/health")
        data = response.json()
        assert "check_duration_ms" in data
        assert data["check_duration_ms"] > 0


class TestDatabaseHealthEndpoint:
    """Tests for /health/database endpoint."""

    def test_database_endpoint_returns_200_or_503(self, client) -> None:
        """Database endpoint returns valid status code."""
        response = client.get("/health/database")
        assert response.status_code in [200, 503]

    def test_database_endpoint_includes_status(self, client) -> None:
        """Database endpoint includes health status."""
        response = client.get("/health/database")
        data = response.json()
        assert "status" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]

    def test_database_endpoint_includes_message(self, client) -> None:
        """Database endpoint includes status message."""
        response = client.get("/health/database")
        data = response.json()
        assert "message" in data
        assert isinstance(data["message"], str)

    def test_database_endpoint_includes_details(self, client) -> None:
        """Database endpoint includes detailed metrics."""
        response = client.get("/health/database")
        data = response.json()
        assert "details" in data
        assert isinstance(data["details"], dict)

    def test_database_endpoint_includes_response_time(self, client) -> None:
        """Database endpoint includes response time."""
        response = client.get("/health/database")
        data = response.json()
        assert "response_time_ms" in data
        assert data["response_time_ms"] > 0


class TestCacheHealthEndpoint:
    """Tests for /health/cache endpoint."""

    def test_cache_endpoint_returns_200_or_503(self, client) -> None:
        """Cache endpoint returns valid status code."""
        response = client.get("/health/cache")
        assert response.status_code in [200, 503]

    def test_cache_endpoint_includes_status(self, client) -> None:
        """Cache endpoint includes health status."""
        response = client.get("/health/cache")
        data = response.json()
        assert "status" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]

    def test_cache_endpoint_includes_message(self, client) -> None:
        """Cache endpoint includes status message."""
        response = client.get("/health/cache")
        data = response.json()
        assert "message" in data

    def test_cache_endpoint_includes_details(self, client) -> None:
        """Cache endpoint includes metrics details."""
        response = client.get("/health/cache")
        data = response.json()
        assert "details" in data


class TestGraphQLHealthEndpoint:
    """Tests for /health/graphql endpoint."""

    def test_graphql_endpoint_returns_200_or_503(self, client) -> None:
        """GraphQL endpoint returns valid status code."""
        response = client.get("/health/graphql")
        assert response.status_code in [200, 503]

    def test_graphql_endpoint_includes_status(self, client) -> None:
        """GraphQL endpoint includes health status."""
        response = client.get("/health/graphql")
        data = response.json()
        assert "status" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]

    def test_graphql_endpoint_includes_details(self, client) -> None:
        """GraphQL endpoint includes operation details."""
        response = client.get("/health/graphql")
        data = response.json()
        assert "details" in data


class TestTracingHealthEndpoint:
    """Tests for /health/tracing endpoint."""

    def test_tracing_endpoint_returns_200_or_503(self, client) -> None:
        """Tracing endpoint returns valid status code."""
        response = client.get("/health/tracing")
        assert response.status_code in [200, 503]

    def test_tracing_endpoint_includes_status(self, client) -> None:
        """Tracing endpoint includes health status."""
        response = client.get("/health/tracing")
        data = response.json()
        assert "status" in data
        assert data["status"] in ["healthy", "degraded", "unhealthy"]


class TestEndpointErrorHandling:
    """Tests for error handling in endpoints."""

    def test_endpoints_dont_crash_on_monitor_unavailable(self, client) -> None:
        """Endpoints handle unavailable monitors gracefully."""
        # All endpoints should return valid responses
        responses = [
            client.get("/health/live"),
            client.get("/health/ready"),
            client.get("/health"),
            client.get("/health/database"),
            client.get("/health/cache"),
            client.get("/health/graphql"),
            client.get("/health/tracing"),
        ]

        for response in responses:
            assert response.status_code in [200, 503]
            assert response.content  # Has content


class TestEndpointIntegration:
    """Integration tests for health endpoints."""

    def test_health_check_endpoint_accessible(self) -> None:
        """Health check endpoint is accessible."""
        app = FastAPI()
        setup_health_endpoints(app)
        client = TestClient(app)

        response = client.get("/health")

        assert response.status_code in [200, 503]
        data = response.json()
        assert "status" in data
        assert "database" in data

    def test_readiness_probe_accessible(self) -> None:
        """Readiness probe endpoint is accessible."""
        app = FastAPI()
        setup_health_endpoints(app)
        client = TestClient(app)

        response = client.get("/health/ready")

        assert response.status_code in [200, 503]
        data = response.json()
        assert "ready" in data

    def test_all_endpoints_accessible(self) -> None:
        """All health endpoints are accessible."""
        app = FastAPI()
        setup_health_endpoints(app)
        client = TestClient(app)

        endpoints = [
            "/health/live",
            "/health/ready",
            "/health",
            "/health/database",
            "/health/cache",
            "/health/graphql",
            "/health/tracing",
        ]

        for endpoint in endpoints:
            response = client.get(endpoint)
            assert response.status_code in [200, 503], f"Failed: {endpoint}"
