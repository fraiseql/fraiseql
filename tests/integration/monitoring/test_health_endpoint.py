"""Integration tests for /health and /ready endpoints.

Tests the Kubernetes liveness and readiness probes.
"""

import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from fraiseql.monitoring.health import HealthCheck, check_database

pytestmark = pytest.mark.integration


@pytest.fixture
def app_with_health() -> None:
    """Create a test FastAPI app with health endpoints."""
    app = FastAPI()

    # Create health checker
    health = HealthCheck()
    health.add_check("database", check_database)

    @app.get("/health")
    async def liveness() -> None:
        """Liveness probe - always returns 200 if app is running."""
        return {"status": "healthy"}

    @app.get("/ready")
    async def readiness() -> None:
        """Readiness probe - returns 200 if app can serve traffic."""
        result = await health.run_checks()
        if result["status"] == "healthy":
            return result
        # Return 503 Service Unavailable if not ready
        from fastapi import Response

        return Response(content=result, status_code=503, media_type="application/json")

    return app


@pytest.mark.asyncio
async def test_health_endpoint_returns_200(app_with_health) -> None:
    """Test that /health endpoint exists and returns 200."""
    async with AsyncClient(
        transport=ASGITransport(app=app_with_health), base_url="http://test"
    ) as client:
        response = await client.get("/health")

    assert response.status_code == 200
    assert response.json() == {"status": "healthy"}


@pytest.mark.asyncio
async def test_ready_endpoint_exists(app_with_health) -> None:
    """Test that /ready endpoint exists (will fail until implemented)."""
    async with AsyncClient(
        transport=ASGITransport(app=app_with_health), base_url="http://test"
    ) as client:
        response = await client.get("/ready")

    # Should return 200 or 503, not 404
    assert response.status_code in [200, 503], "Ready endpoint should exist"


@pytest.mark.asyncio
async def test_ready_endpoint_checks_database(app_with_health) -> None:
    """Test that /ready endpoint performs database connectivity check."""
    async with AsyncClient(
        transport=ASGITransport(app=app_with_health), base_url="http://test"
    ) as client:
        response = await client.get("/ready")

    # Should have checks in response
    data = response.json()
    assert "checks" in data or "status" in data


@pytest.mark.asyncio
async def test_healthcheck_class_exists() -> None:
    """Test that HealthCheck class can be imported and instantiated."""
    # This will fail until we create the module
    from fraiseql.monitoring.health import HealthCheck

    health = HealthCheck()
    assert health is not None


@pytest.mark.asyncio
async def test_healthcheck_add_check() -> None:
    """Test that checks can be added to HealthCheck."""
    from fraiseql.monitoring.health import HealthCheck

    health = HealthCheck()

    async def dummy_check() -> None:
        return {"status": "ok"}

    health.add_check("test", dummy_check)

    # Should have the check registered
    result = await health.run_checks()
    assert "checks" in result


@pytest.mark.asyncio
async def test_check_database_function_exists() -> None:
    """Test that check_database helper function exists."""
    from fraiseql.monitoring.health import check_database

    assert callable(check_database)
