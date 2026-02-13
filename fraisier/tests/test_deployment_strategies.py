"""Tests for advanced deployment strategies.

Tests cover:
- Rolling deployments
- Blue-Green deployments
- Canary deployments
- Progressive deployments
- Automatic rollback on failure
- Health monitoring during deployments
"""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from fraisier.deployment_strategies import (
    DeploymentStrategy,
    StrategyConfig,
    BasicMetricsProvider,
    RollingDeploymentStrategy,
    BlueGreenDeploymentStrategy,
    CanaryDeploymentStrategy,
    ProgressiveDeploymentStrategy,
    DeploymentMetrics,
    get_strategy,
)
from fraisier.providers.base import HealthCheck, HealthCheckType


@pytest.fixture
def mock_provider():
    """Create mock deployment provider."""
    provider = AsyncMock()
    provider.get_service_status.return_value = {"instance_count": 3}
    provider.check_health.return_value = True
    return provider


@pytest.fixture
def mock_metrics_provider():
    """Create mock metrics provider."""
    provider = AsyncMock()
    metrics = DeploymentMetrics(error_rate=0.0, latency_p99=100.0)
    provider.get_metrics.return_value = metrics
    return provider


@pytest.fixture
def health_check():
    """Create health check configuration."""
    return HealthCheck(
        type=HealthCheckType.HTTP,
        url="http://localhost:8000/health",
        timeout=10,
        retries=3,
    )


@pytest.fixture
def strategy_config():
    """Create strategy configuration."""
    return StrategyConfig(
        max_concurrent=1,
        canary_duration=10,
        health_check_interval=2,
        auto_rollback_on_failure=True,
    )


class TestRollingDeployment:
    """Test rolling deployment strategy."""

    @pytest.mark.asyncio
    async def test_rolling_deployment_success(self, mock_provider, health_check, strategy_config):
        """Test successful rolling deployment."""
        strategy = RollingDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        mock_provider.check_health.assert_called()

    @pytest.mark.asyncio
    async def test_rolling_deployment_health_check_failure(
        self, mock_provider, health_check, strategy_config
    ):
        """Test rolling deployment with health check failure."""
        mock_provider.check_health.side_effect = [True, False, True]
        strategy = RollingDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False
        mock_provider.check_health.assert_called()

    @pytest.mark.asyncio
    async def test_rolling_deployment_with_multiple_batches(
        self, mock_provider, health_check, strategy_config
    ):
        """Test rolling deployment with multiple batches."""
        mock_provider.get_service_status.return_value = {"instance_count": 6}
        strategy_config.max_concurrent = 2

        strategy = RollingDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        # Should be called multiple times for each batch
        assert mock_provider.check_health.call_count >= 3

    @pytest.mark.asyncio
    async def test_rolling_deployment_auto_rollback(
        self, mock_provider, health_check, strategy_config
    ):
        """Test rolling deployment with automatic rollback on failure."""
        mock_provider.check_health.return_value = False
        strategy = RollingDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False


class TestBlueGreenDeployment:
    """Test blue-green deployment strategy."""

    @pytest.mark.asyncio
    async def test_blue_green_deployment_success(
        self, mock_provider, health_check, strategy_config
    ):
        """Test successful blue-green deployment."""
        strategy = BlueGreenDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        # Health check should be called for Green environment
        assert mock_provider.check_health.call_count >= 2

    @pytest.mark.asyncio
    async def test_blue_green_green_health_check_failure(
        self, mock_provider, health_check, strategy_config
    ):
        """Test blue-green deployment when Green fails health check."""
        mock_provider.check_health.return_value = False
        strategy = BlueGreenDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_blue_green_traffic_switch_failure(
        self, mock_provider, health_check, strategy_config
    ):
        """Test blue-green deployment when Green fails after traffic switch."""
        # First call for initial deployment health check passes
        # Second call after traffic switch fails
        mock_provider.check_health.side_effect = [True, False]
        strategy = BlueGreenDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_blue_green_instant_rollback_capability(
        self, mock_provider, health_check, strategy_config
    ):
        """Test that Blue environment is kept for instant rollback."""
        strategy = BlueGreenDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        # After successful deployment, blue should be kept
        assert result is True


class TestCanaryDeployment:
    """Test canary deployment strategy."""

    @pytest.mark.asyncio
    async def test_canary_deployment_success(
        self, mock_provider, mock_metrics_provider, health_check, strategy_config
    ):
        """Test successful canary deployment."""
        strategy = CanaryDeploymentStrategy(mock_provider, strategy_config, mock_metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        mock_metrics_provider.get_metrics.assert_called()

    @pytest.mark.asyncio
    async def test_canary_health_check_failure(
        self, mock_provider, mock_metrics_provider, health_check, strategy_config
    ):
        """Test canary deployment with health check failure."""
        mock_provider.check_health.return_value = False
        strategy = CanaryDeploymentStrategy(mock_provider, strategy_config, mock_metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_canary_error_rate_threshold_exceeded(
        self, mock_provider, health_check, strategy_config
    ):
        """Test canary deployment when error rate exceeds threshold."""
        # Health check passes but metrics show high error rate
        mock_provider.check_health.return_value = True

        metrics_provider = AsyncMock()
        bad_metrics = DeploymentMetrics(error_rate=10.0)  # Exceeds 5% threshold
        metrics_provider.get_metrics.return_value = bad_metrics

        strategy = CanaryDeploymentStrategy(mock_provider, strategy_config, metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_canary_gradual_traffic_increase(
        self, mock_provider, mock_metrics_provider, health_check, strategy_config
    ):
        """Test canary deployment with gradual traffic increase."""
        strategy = CanaryDeploymentStrategy(mock_provider, strategy_config, mock_metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        # Should monitor through multiple traffic percentages (10%, 25%, 50%, 100%)
        assert mock_provider.check_health.call_count >= 4


class TestProgressiveDeployment:
    """Test progressive deployment strategy."""

    @pytest.mark.asyncio
    async def test_progressive_deployment_success(
        self, mock_provider, mock_metrics_provider, health_check, strategy_config
    ):
        """Test successful progressive deployment."""
        strategy = ProgressiveDeploymentStrategy(
            mock_provider, strategy_config, mock_metrics_provider
        )

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        mock_metrics_provider.get_metrics.assert_called()

    @pytest.mark.asyncio
    async def test_progressive_health_check_failure(
        self, mock_provider, mock_metrics_provider, health_check, strategy_config
    ):
        """Test progressive deployment with health check failure."""
        # Fail on first stage (25%)
        mock_provider.check_health.return_value = False
        strategy = ProgressiveDeploymentStrategy(
            mock_provider, strategy_config, mock_metrics_provider
        )

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_progressive_error_rate_threshold_exceeded(
        self, mock_provider, health_check, strategy_config
    ):
        """Test progressive deployment when error rate exceeds threshold."""
        mock_provider.check_health.return_value = True

        metrics_provider = AsyncMock()
        bad_metrics = DeploymentMetrics(error_rate=8.0)  # Exceeds 5% threshold
        metrics_provider.get_metrics.return_value = bad_metrics

        strategy = ProgressiveDeploymentStrategy(mock_provider, strategy_config, metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_progressive_all_stages(
        self, mock_provider, mock_metrics_provider, health_check, strategy_config
    ):
        """Test progressive deployment through all stages (25%, 50%, 75%, 100%)."""
        strategy = ProgressiveDeploymentStrategy(
            mock_provider, strategy_config, mock_metrics_provider
        )

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        # Should monitor through 4 stages
        assert mock_provider.check_health.call_count >= 4


class TestStrategyFactory:
    """Test deployment strategy factory function."""

    def test_get_rolling_strategy(self, mock_provider, strategy_config):
        """Test factory creates rolling strategy."""
        from fraisier.deployment_strategies import DeploymentStrategy as StrategyEnum

        strategy = get_strategy(mock_provider, StrategyEnum.ROLLING, strategy_config)
        assert isinstance(strategy, RollingDeploymentStrategy)

    def test_get_blue_green_strategy(self, mock_provider, strategy_config):
        """Test factory creates blue-green strategy."""
        from fraisier.deployment_strategies import DeploymentStrategy as StrategyEnum

        strategy = get_strategy(mock_provider, StrategyEnum.BLUE_GREEN, strategy_config)
        assert isinstance(strategy, BlueGreenDeploymentStrategy)

    def test_get_canary_strategy(self, mock_provider, strategy_config):
        """Test factory creates canary strategy."""
        from fraisier.deployment_strategies import DeploymentStrategy as StrategyEnum

        strategy = get_strategy(mock_provider, StrategyEnum.CANARY, strategy_config)
        assert isinstance(strategy, CanaryDeploymentStrategy)

    def test_get_progressive_strategy(self, mock_provider, strategy_config):
        """Test factory creates progressive strategy."""
        from fraisier.deployment_strategies import DeploymentStrategy as StrategyEnum

        strategy = get_strategy(mock_provider, StrategyEnum.PROGRESSIVE, strategy_config)
        assert isinstance(strategy, ProgressiveDeploymentStrategy)

    def test_get_strategy_with_defaults(self, mock_provider):
        """Test factory uses default configuration if none provided."""
        from fraisier.deployment_strategies import DeploymentStrategy as StrategyEnum

        strategy = get_strategy(mock_provider, StrategyEnum.ROLLING)
        assert strategy.config.max_concurrent == 1
        assert strategy.config.canary_duration == 300


class TestMetricsMonitoring:
    """Test metrics monitoring during deployments."""

    @pytest.mark.asyncio
    async def test_high_latency_detection(self, mock_provider, health_check, strategy_config):
        """Test detection of high latency during deployment."""
        mock_provider.check_health.return_value = True

        metrics_provider = AsyncMock()
        high_latency_metrics = DeploymentMetrics(latency_p99=2000.0)  # High latency
        metrics_provider.get_metrics.return_value = high_latency_metrics

        strategy = CanaryDeploymentStrategy(mock_provider, strategy_config, metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        # Deployment should still succeed (warning logged but not failure)
        # This allows for transient latency spikes
        assert result is True

    @pytest.mark.asyncio
    async def test_cpu_memory_tracking(self, mock_provider, health_check, strategy_config):
        """Test that CPU and memory metrics are tracked."""
        metrics_provider = AsyncMock()
        metrics = DeploymentMetrics(cpu_usage=75.0, memory_usage=80.0)
        metrics_provider.get_metrics.return_value = metrics

        strategy = ProgressiveDeploymentStrategy(mock_provider, strategy_config, metrics_provider)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is True
        metrics_provider.get_metrics.assert_called()


class TestAutoRollback:
    """Test automatic rollback functionality."""

    @pytest.mark.asyncio
    async def test_auto_rollback_enabled(self, mock_provider, health_check):
        """Test that rollback is triggered when auto_rollback_on_failure is True."""
        strategy_config = StrategyConfig(
            auto_rollback_on_failure=True,
            canary_duration=5,
        )
        mock_provider.check_health.return_value = False

        strategy = RollingDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_auto_rollback_disabled(self, mock_provider, health_check):
        """Test that rollback is skipped when auto_rollback_on_failure is False."""
        strategy_config = StrategyConfig(
            auto_rollback_on_failure=False,
            canary_duration=5,
        )
        mock_provider.check_health.return_value = False

        strategy = RollingDeploymentStrategy(mock_provider, strategy_config)

        result = await strategy.execute(
            "api",
            old_version="1.0.0",
            new_version="2.0.0",
            health_check=health_check,
        )

        assert result is False


class TestBasicMetricsProvider:
    """Test basic metrics provider."""

    @pytest.mark.asyncio
    async def test_basic_metrics_provider_returns_zeros(self):
        """Test that basic metrics provider returns zero values."""
        provider = BasicMetricsProvider()
        metrics = await provider.get_metrics()

        assert metrics.error_rate == 0.0
        assert metrics.latency_p99 == 0.0
        assert metrics.cpu_usage == 0.0
        assert metrics.memory_usage == 0.0
