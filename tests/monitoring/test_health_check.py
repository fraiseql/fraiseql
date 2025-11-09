"""Tests for HealthCheck utility class."""

import pytest

from fraiseql.monitoring.health import CheckResult, HealthCheck, HealthStatus


class TestHealthCheckCore:
    """Test core HealthCheck functionality."""

    def test_healthcheck_instantiation(self) -> None:
        """Test that HealthCheck can be instantiated."""
        health = HealthCheck()
        assert health is not None

    def test_healthcheck_add_check(self) -> None:
        """Test adding a custom check function."""
        health = HealthCheck()

        # Define a simple passing check
        async def dummy_check() -> CheckResult:
            return CheckResult(
                name="dummy",
                status=HealthStatus.HEALTHY,
                message="All good",
            )

        # Should be able to add a check
        health.add_check("dummy", dummy_check)
        assert "dummy" in health._checks

    def test_healthcheck_duplicate_check_raises(self) -> None:
        """Test that adding duplicate check name raises ValueError."""
        health = HealthCheck()

        async def check_1() -> CheckResult:
            return CheckResult(
                name="test",
                status=HealthStatus.HEALTHY,
                message="OK",
            )

        async def check_2() -> CheckResult:
            return CheckResult(
                name="test",
                status=HealthStatus.HEALTHY,
                message="OK",
            )

        health.add_check("test", check_1)

        # Should raise ValueError when adding duplicate
        with pytest.raises(ValueError, match="already registered"):
            health.add_check("test", check_2)

    @pytest.mark.asyncio
    async def test_healthcheck_run_single_check(self) -> None:
        """Test running a single health check."""
        health = HealthCheck()

        async def passing_check() -> CheckResult:
            return CheckResult(
                name="test",
                status=HealthStatus.HEALTHY,
                message="OK",
            )

        health.add_check("test", passing_check)
        result = await health.run_checks()

        assert result["status"] == "healthy"
        assert "test" in result["checks"]
        assert result["checks"]["test"]["status"] == "healthy"

    @pytest.mark.asyncio
    async def test_healthcheck_run_multiple_checks(self) -> None:
        """Test running multiple health checks."""
        health = HealthCheck()

        async def check_1() -> CheckResult:
            return CheckResult(
                name="check1",
                status=HealthStatus.HEALTHY,
                message="OK",
            )

        async def check_2() -> CheckResult:
            return CheckResult(
                name="check2",
                status=HealthStatus.HEALTHY,
                message="OK",
            )

        health.add_check("check1", check_1)
        health.add_check("check2", check_2)

        result = await health.run_checks()

        assert result["status"] == "healthy"
        assert len(result["checks"]) == 2
        assert "check1" in result["checks"]
        assert "check2" in result["checks"]

    @pytest.mark.asyncio
    async def test_healthcheck_degraded_when_check_fails(self) -> None:
        """Test that overall status is degraded when any check fails."""
        health = HealthCheck()

        async def passing_check() -> CheckResult:
            return CheckResult(
                name="good",
                status=HealthStatus.HEALTHY,
                message="OK",
            )

        async def failing_check() -> CheckResult:
            return CheckResult(
                name="bad",
                status=HealthStatus.UNHEALTHY,
                message="Database connection failed",
            )

        health.add_check("good", passing_check)
        health.add_check("bad", failing_check)

        result = await health.run_checks()

        # Overall status should be degraded if any check fails
        assert result["status"] == "degraded"
        assert result["checks"]["good"]["status"] == "healthy"
        assert result["checks"]["bad"]["status"] == "unhealthy"

    @pytest.mark.asyncio
    async def test_healthcheck_exception_handling(self) -> None:
        """Test that exceptions in checks are caught and reported."""
        health = HealthCheck()

        async def broken_check() -> CheckResult:
            raise Exception("Something went wrong!")

        health.add_check("broken", broken_check)
        result = await health.run_checks()

        # Should catch exception and report as unhealthy
        assert result["status"] == "degraded"
        assert result["checks"]["broken"]["status"] == "unhealthy"
        assert "Something went wrong!" in result["checks"]["broken"]["message"]


class TestCheckResult:
    """Test CheckResult data structure."""

    def test_check_result_creation(self) -> None:
        """Test creating a CheckResult."""
        result = CheckResult(
            name="test",
            status=HealthStatus.HEALTHY,
            message="All systems operational",
        )
        assert result.name == "test"
        assert result.status == HealthStatus.HEALTHY
        assert result.message == "All systems operational"

    def test_check_result_with_metadata(self) -> None:
        """Test CheckResult with optional metadata."""
        result = CheckResult(
            name="database",
            status=HealthStatus.HEALTHY,
            message="Connected",
            metadata={"pool_size": 10, "active_connections": 3},
        )
        assert result.metadata["pool_size"] == 10
        assert result.metadata["active_connections"] == 3


class TestHealthStatus:
    """Test HealthStatus enum."""

    def test_health_status_values(self) -> None:
        """Test that HealthStatus enum has expected values."""
        assert HealthStatus.HEALTHY.value == "healthy"
        assert HealthStatus.UNHEALTHY.value == "unhealthy"
        assert HealthStatus.DEGRADED.value == "degraded"
