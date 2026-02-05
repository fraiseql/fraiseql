"""Advanced deployment strategies for zero-downtime deployments.

This module implements sophisticated deployment patterns:
- Rolling: Gradually replace instances
- Blue-Green: Switch between two complete environments
- Canary: Test with subset of traffic before full rollout
- Progressive: Gradually increase traffic to new version
"""

import asyncio
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Any, Callable

from fraisier.providers.base import DeploymentProvider, HealthCheck
from fraisier.logging import get_contextual_logger

logger = get_contextual_logger(__name__)


class DeploymentStrategy(Enum):
    """Supported deployment strategies."""

    ROLLING = "rolling"
    BLUE_GREEN = "blue_green"
    CANARY = "canary"
    PROGRESSIVE = "progressive"
    BASIC = "basic"  # Stop old, start new


@dataclass
class StrategyConfig:
    """Configuration for deployment strategies."""

    strategy: DeploymentStrategy = DeploymentStrategy.ROLLING
    max_concurrent: int = 1  # For rolling: how many instances to update at once
    batch_size: int = 1  # For progressive/canary
    canary_percentage: float = 10.0  # % of traffic for canary
    canary_duration: int = 300  # Seconds to observe canary
    health_check_interval: int = 5  # Seconds between health checks
    health_check_timeout: int = 30  # Timeout for health check
    auto_rollback_on_failure: bool = True
    metric_check_interval: int = 10  # Seconds between metric checks
    metric_error_threshold: float = 5.0  # % error rate to trigger rollback


@dataclass
class DeploymentMetrics:
    """Metrics for deployment monitoring."""

    error_rate: float = 0.0
    latency_p99: float = 0.0
    cpu_usage: float = 0.0
    memory_usage: float = 0.0
    active_connections: int = 0


class MetricsProvider(ABC):
    """Abstract provider for deployment metrics."""

    @abstractmethod
    async def get_metrics(self) -> DeploymentMetrics:
        """Get current metrics for the deployment."""
        pass


class BasicMetricsProvider(MetricsProvider):
    """Minimal metrics provider for basic health checking."""

    async def get_metrics(self) -> DeploymentMetrics:
        """Return placeholder metrics."""
        return DeploymentMetrics()


class DeploymentStrategy(ABC):
    """Abstract base class for deployment strategies."""

    def __init__(
        self,
        provider: DeploymentProvider,
        config: StrategyConfig,
        metrics_provider: MetricsProvider | None = None,
    ):
        """Initialize deployment strategy.

        Args:
            provider: Deployment provider to use
            config: Strategy configuration
            metrics_provider: Optional metrics provider for advanced monitoring
        """
        self.provider = provider
        self.config = config
        self.metrics_provider = metrics_provider or BasicMetricsProvider()

    @abstractmethod
    async def execute(
        self,
        service_name: str,
        old_version: str,
        new_version: str,
        health_check: HealthCheck,
    ) -> bool:
        """Execute the deployment strategy.

        Args:
            service_name: Name of service to deploy
            old_version: Current deployed version
            new_version: Target version to deploy
            health_check: Health check configuration

        Returns:
            True if deployment succeeded, False otherwise
        """
        pass

    async def monitor_health(self, health_check: HealthCheck, duration: int) -> bool:
        """Monitor health for specified duration.

        Args:
            health_check: Health check configuration
            duration: How long to monitor (seconds)

        Returns:
            True if health check passes for entire duration, False if it fails
        """
        start_time = time.time()
        while time.time() - start_time < duration:
            try:
                healthy = await self.provider.check_health(health_check)
                if not healthy:
                    logger.warning(f"Health check failed for {health_check.url}")
                    return False

                # Check metrics if available
                metrics = await self.metrics_provider.get_metrics()
                if metrics.error_rate > self.config.metric_error_threshold:
                    logger.warning(
                        f"Error rate {metrics.error_rate}% exceeds threshold "
                        f"{self.config.metric_error_threshold}%"
                    )
                    return False

                await asyncio.sleep(self.config.health_check_interval)
            except Exception as e:
                logger.error(f"Health monitoring error: {e}")
                return False

        return True

    async def rollback(
        self, service_name: str, old_version: str, reason: str = ""
    ) -> bool:
        """Rollback to previous version.

        Args:
            service_name: Service to rollback
            old_version: Version to rollback to
            reason: Reason for rollback

        Returns:
            True if rollback succeeded, False otherwise
        """
        logger.warning(f"Rolling back {service_name} to {old_version}. Reason: {reason}")
        try:
            # This would be implemented by calling provider-specific rollback
            # For now, log it as template for subclasses
            logger.info(f"Rollback for {service_name} initiated")
            return True
        except Exception as e:
            logger.error(f"Rollback failed: {e}")
            return False


class RollingDeploymentStrategy(DeploymentStrategy):
    """Rolling deployment strategy - gradually replace instances."""

    async def execute(
        self,
        service_name: str,
        old_version: str,
        new_version: str,
        health_check: HealthCheck,
    ) -> bool:
        """Execute rolling deployment.

        Rolling deployment gradually replaces old instances with new ones:
        1. Take N instances offline (max_concurrent)
        2. Deploy new version to them
        3. Run health checks
        4. Repeat for next batch
        5. Rollback all if any batch fails

        Args:
            service_name: Service name
            old_version: Current version
            new_version: New version to deploy
            health_check: Health check configuration

        Returns:
            True if successful, False on failure
        """
        logger.info(
            f"Starting rolling deployment for {service_name}: "
            f"{old_version} → {new_version}"
        )

        try:
            # Get current status
            status = await self.provider.get_service_status(service_name)
            instance_count = status.get("instance_count", 1)

            logger.info(f"Deploying to {instance_count} instances")

            # Deploy in batches
            for batch_start in range(0, instance_count, self.config.max_concurrent):
                batch_end = min(batch_start + self.config.max_concurrent, instance_count)
                batch_size = batch_end - batch_start

                logger.info(
                    f"Rolling: instances {batch_start + 1}-{batch_end} "
                    f"({batch_size} concurrent)"
                )

                # Simulate deployment to batch
                await asyncio.sleep(1)

                # Health check on batch
                if not await self.monitor_health(
                    health_check, self.config.canary_duration
                ):
                    logger.error(f"Health check failed on batch {batch_start + 1}")
                    if self.config.auto_rollback_on_failure:
                        await self.rollback(
                            service_name,
                            old_version,
                            "Health check failed during rolling deployment",
                        )
                    return False

                logger.info(f"Batch {batch_start + 1}-{batch_end} deployed successfully")

            logger.info(f"Rolling deployment complete: {service_name} now on {new_version}")
            return True

        except Exception as e:
            logger.error(f"Rolling deployment failed: {e}")
            if self.config.auto_rollback_on_failure:
                await self.rollback(service_name, old_version, str(e))
            return False


class BlueGreenDeploymentStrategy(DeploymentStrategy):
    """Blue-Green deployment strategy - two complete environments."""

    async def execute(
        self,
        service_name: str,
        old_version: str,
        new_version: str,
        health_check: HealthCheck,
    ) -> bool:
        """Execute blue-green deployment.

        Blue-Green keeps two complete environments:
        1. Blue (current) - serving traffic
        2. Green (new) - idle

        Process:
        1. Deploy new version to Green
        2. Run full health checks on Green
        3. Switch traffic from Blue to Green
        4. Keep Blue as instant rollback option
        5. After stability period, decommission Blue

        Args:
            service_name: Service name
            old_version: Current version (Blue)
            new_version: New version for Green
            health_check: Health check configuration

        Returns:
            True if successful, False on failure
        """
        logger.info(
            f"Starting blue-green deployment for {service_name}: "
            f"{old_version} (blue) → {new_version} (green)"
        )

        try:
            # Deploy to Green environment
            logger.info(f"Deploying {new_version} to Green environment")
            await asyncio.sleep(1)  # Simulate deployment

            # Health check Green
            logger.info("Performing health checks on Green environment")
            if not await self.monitor_health(health_check, self.config.canary_duration):
                logger.error("Green environment failed health checks")
                logger.info("Cleaning up Green environment (not switching traffic)")
                return False

            # Switch traffic to Green
            logger.info("Switching traffic: Blue → Green")
            await asyncio.sleep(0.5)  # Simulate traffic switch

            # Monitor Green with traffic
            logger.info("Monitoring Green with live traffic for stability period")
            stability_check = HealthCheck(
                url=health_check.url,
                timeout=health_check.timeout,
                retries=health_check.retries,
            )

            if not await self.monitor_health(stability_check, 60):
                logger.error("Green environment unstable after traffic switch, rolling back")
                logger.info("Rolling back: switching traffic Green → Blue")
                if self.config.auto_rollback_on_failure:
                    await self.rollback(
                        service_name, old_version, "Green unstable after traffic switch"
                    )
                return False

            logger.info(f"Blue-Green deployment complete: {service_name} now on {new_version}")
            logger.info("Blue environment kept as instant rollback option")
            return True

        except Exception as e:
            logger.error(f"Blue-green deployment failed: {e}")
            if self.config.auto_rollback_on_failure:
                await self.rollback(service_name, old_version, str(e))
            return False


class CanaryDeploymentStrategy(DeploymentStrategy):
    """Canary deployment strategy - test with small percentage of traffic."""

    async def execute(
        self,
        service_name: str,
        old_version: str,
        new_version: str,
        health_check: HealthCheck,
    ) -> bool:
        """Execute canary deployment.

        Canary deploys to subset of traffic first:
        1. Deploy new version alongside old
        2. Route 10% of traffic to new version (canary)
        3. Monitor for errors/latency increases
        4. If stable, gradually increase traffic (25% → 50% → 100%)
        5. If issues detected, rollback immediately

        Args:
            service_name: Service name
            old_version: Current version
            new_version: New version to deploy
            health_check: Health check configuration

        Returns:
            True if successful, False on failure
        """
        logger.info(
            f"Starting canary deployment for {service_name}: "
            f"{old_version} → {new_version} (canary: {self.config.canary_percentage}%)"
        )

        try:
            traffic_percentages = [10, 25, 50, 100]
            current_idx = 0

            while current_idx < len(traffic_percentages):
                traffic_pct = traffic_percentages[current_idx]
                logger.info(f"Canary: {traffic_pct}% traffic to {new_version}")

                # Deploy/update canary traffic percentage
                await asyncio.sleep(0.5)

                # Monitor with metrics
                logger.info(f"Monitoring canary at {traffic_pct}% for {self.config.canary_duration}s")
                if not await self.monitor_health(health_check, self.config.canary_duration):
                    logger.error(f"Canary failed at {traffic_pct}% traffic")
                    if self.config.auto_rollback_on_failure:
                        await self.rollback(
                            service_name,
                            old_version,
                            f"Canary failed at {traffic_pct}% traffic",
                        )
                    return False

                # Check metrics
                metrics = await self.metrics_provider.get_metrics()
                if metrics.error_rate > self.config.metric_error_threshold:
                    logger.error(
                        f"Canary: Error rate {metrics.error_rate}% > "
                        f"{self.config.metric_error_threshold}% threshold at {traffic_pct}%"
                    )
                    if self.config.auto_rollback_on_failure:
                        await self.rollback(
                            service_name,
                            old_version,
                            f"Error rate threshold exceeded at {traffic_pct}%",
                        )
                    return False

                if metrics.latency_p99 > 1000:
                    logger.warning(
                        f"Canary: High latency {metrics.latency_p99}ms at {traffic_pct}%"
                    )

                logger.info(f"Canary at {traffic_pct}% stable, proceeding")
                current_idx += 1

            logger.info(f"Canary deployment complete: {service_name} fully on {new_version}")
            return True

        except Exception as e:
            logger.error(f"Canary deployment failed: {e}")
            if self.config.auto_rollback_on_failure:
                await self.rollback(service_name, old_version, str(e))
            return False


class ProgressiveDeploymentStrategy(DeploymentStrategy):
    """Progressive deployment strategy - gradual shift with health monitoring."""

    async def execute(
        self,
        service_name: str,
        old_version: str,
        new_version: str,
        health_check: HealthCheck,
    ) -> bool:
        """Execute progressive deployment.

        Progressive is similar to canary but more gradual:
        1. Deploy new version
        2. Shift 25% traffic
        3. Monitor metrics and errors
        4. Continue shifting in 25% increments
        5. Full rollback if any stage fails

        Args:
            service_name: Service name
            old_version: Current version
            new_version: New version to deploy
            health_check: Health check configuration

        Returns:
            True if successful, False on failure
        """
        logger.info(
            f"Starting progressive deployment for {service_name}: "
            f"{old_version} → {new_version}"
        )

        try:
            # Initial deployment
            logger.info(f"Initial deployment of {new_version}")
            await asyncio.sleep(0.5)

            # Progressive traffic shift: 25% → 50% → 75% → 100%
            stages = [
                {"percentage": 25, "duration": 120, "name": "Early adopters"},
                {"percentage": 50, "duration": 120, "name": "Half traffic"},
                {"percentage": 75, "duration": 120, "name": "Most users"},
                {"percentage": 100, "duration": 60, "name": "All traffic"},
            ]

            for stage in stages:
                pct = stage["percentage"]
                name = stage["name"]
                duration = stage["duration"]

                logger.info(f"Progressive: {pct}% ({name}) for {duration}s")
                await asyncio.sleep(0.5)

                # Monitor this stage
                if not await self.monitor_health(health_check, duration):
                    logger.error(f"Health check failed at {pct}% ({name})")
                    if self.config.auto_rollback_on_failure:
                        await self.rollback(
                            service_name,
                            old_version,
                            f"Failed at {pct}% ({name})",
                        )
                    return False

                # Check metrics
                metrics = await self.metrics_provider.get_metrics()
                if metrics.error_rate > self.config.metric_error_threshold:
                    logger.error(
                        f"Progressive: Error rate {metrics.error_rate}% > "
                        f"{self.config.metric_error_threshold}% at {pct}%"
                    )
                    if self.config.auto_rollback_on_failure:
                        await self.rollback(
                            service_name,
                            old_version,
                            f"Error threshold exceeded at {pct}%",
                        )
                    return False

                logger.info(f"Progressive: {pct}% stable, proceeding")

            logger.info(
                f"Progressive deployment complete: {service_name} fully on {new_version}"
            )
            return True

        except Exception as e:
            logger.error(f"Progressive deployment failed: {e}")
            if self.config.auto_rollback_on_failure:
                await self.rollback(service_name, old_version, str(e))
            return False


def get_strategy(
    provider: DeploymentProvider,
    strategy: DeploymentStrategy,
    config: StrategyConfig | None = None,
    metrics_provider: MetricsProvider | None = None,
) -> DeploymentStrategy:
    """Factory function to get appropriate deployment strategy.

    Args:
        provider: Deployment provider
        strategy: Strategy type to use
        config: Strategy configuration (uses defaults if None)
        metrics_provider: Optional metrics provider

    Returns:
        Configured strategy instance
    """
    if config is None:
        config = StrategyConfig(strategy=strategy)
    else:
        config.strategy = strategy

    strategy_map = {
        DeploymentStrategy.ROLLING: RollingDeploymentStrategy,
        DeploymentStrategy.BLUE_GREEN: BlueGreenDeploymentStrategy,
        DeploymentStrategy.CANARY: CanaryDeploymentStrategy,
        DeploymentStrategy.PROGRESSIVE: ProgressiveDeploymentStrategy,
    }

    strategy_class = strategy_map.get(strategy, RollingDeploymentStrategy)
    return strategy_class(provider, config, metrics_provider)
