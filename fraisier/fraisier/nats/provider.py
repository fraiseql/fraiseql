"""NATS event emission mixin for deployment providers.

Adds event publishing capabilities to all deployment providers,
emitting events at key lifecycle points during deployments.
"""

from datetime import datetime, timezone
from typing import Any, TYPE_CHECKING

from fraisier.logging import get_contextual_logger
from fraisier.nats.events import (
    DeploymentEvents,
    HealthCheckEvents,
)

if TYPE_CHECKING:
    from fraisier.nats.client import NatsEventBus

logger = get_contextual_logger(__name__)


class NatsEventProvider:
    """Mixin to add NATS event publishing to deployment providers.

    Providers that inherit from this mixin can emit events at key
    lifecycle points (deployment started, health checks, completion).

    Usage:
        class BareMetalProvider(DeploymentProvider, NatsEventProvider):
            def __init__(self, config, event_bus):
                super().__init__(config)
                self.event_bus = event_bus
    """

    event_bus: "NatsEventBus | None" = None
    region: str | None = None

    async def emit_deployment_started(
        self,
        deployment_id: str,
        service_name: str,
        version: str | None = None,
        strategy: str | None = None,
        trace_id: str | None = None,
    ) -> None:
        """Emit deployment started event.

        Args:
            deployment_id: Unique deployment ID
            service_name: Name of service being deployed
            version: Version being deployed
            strategy: Deployment strategy (rolling, blue-green, etc.)
            trace_id: Optional trace ID for correlation
        """
        if not self.event_bus:
            logger.debug("Event bus not available, skipping deployment_started event")
            return

        try:
            await self.event_bus.publish_deployment_event(
                event_type=DeploymentEvents.STARTED,
                deployment_id=deployment_id,
                data={
                    "service": service_name,
                    "provider": self.__class__.__name__,
                    "version": version,
                    "strategy": strategy,
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                },
                trace_id=trace_id,
                region=self.region,
            )
            logger.debug(f"Emitted deployment_started for {deployment_id}")
        except Exception as e:
            logger.warning(f"Failed to emit deployment_started event: {e}")

    async def emit_deployment_completed(
        self,
        deployment_id: str,
        service_name: str,
        status: str,
        duration_seconds: float,
        version: str | None = None,
        error: str | None = None,
        trace_id: str | None = None,
    ) -> None:
        """Emit deployment completed event.

        Args:
            deployment_id: Unique deployment ID
            service_name: Name of service
            status: Status (success, failure)
            duration_seconds: Total deployment duration
            version: Version that was deployed
            error: Error message if failed
            trace_id: Optional trace ID
        """
        if not self.event_bus:
            logger.debug("Event bus not available, skipping deployment_completed event")
            return

        try:
            event_type = DeploymentEvents.COMPLETED if status == "success" else DeploymentEvents.FAILED

            await self.event_bus.publish_deployment_event(
                event_type=event_type,
                deployment_id=deployment_id,
                data={
                    "service": service_name,
                    "provider": self.__class__.__name__,
                    "version": version,
                    "status": status,
                    "duration_seconds": duration_seconds,
                    "error": error,
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                },
                trace_id=trace_id,
                region=self.region,
            )
            logger.debug(f"Emitted deployment_completed for {deployment_id}")
        except Exception as e:
            logger.warning(f"Failed to emit deployment_completed event: {e}")

    async def emit_health_check_started(
        self,
        service_name: str,
        check_type: str,
        endpoint: str | None = None,
        trace_id: str | None = None,
    ) -> None:
        """Emit health check started event.

        Args:
            service_name: Service being checked
            check_type: Type of check (http, tcp, exec, systemd)
            endpoint: Endpoint being checked
            trace_id: Optional trace ID
        """
        if not self.event_bus:
            return

        try:
            await self.event_bus.publish_health_check_event(
                event_type=HealthCheckEvents.CHECK_STARTED,
                service_name=service_name,
                data={
                    "provider": self.__class__.__name__,
                    "check_type": check_type,
                    "endpoint": endpoint,
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                },
                trace_id=trace_id,
            )
        except Exception as e:
            logger.warning(f"Failed to emit health_check_started event: {e}")

    async def emit_health_check_passed(
        self,
        service_name: str,
        check_type: str,
        duration_ms: int,
        details: dict[str, Any] | None = None,
        trace_id: str | None = None,
    ) -> None:
        """Emit health check passed event.

        Args:
            service_name: Service that passed check
            check_type: Type of check (http, tcp, exec, systemd)
            duration_ms: Check duration in milliseconds
            details: Optional additional details
            trace_id: Optional trace ID
        """
        if not self.event_bus:
            return

        try:
            data = {
                "provider": self.__class__.__name__,
                "check_type": check_type,
                "duration_ms": duration_ms,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            }
            if details:
                data["details"] = details

            await self.event_bus.publish_health_check_event(
                event_type=HealthCheckEvents.CHECK_PASSED,
                service_name=service_name,
                data=data,
                trace_id=trace_id,
            )
            logger.debug(f"Emitted health_check_passed for {service_name}")
        except Exception as e:
            logger.warning(f"Failed to emit health_check_passed event: {e}")

    async def emit_health_check_failed(
        self,
        service_name: str,
        check_type: str,
        reason: str,
        duration_ms: int,
        details: dict[str, Any] | None = None,
        trace_id: str | None = None,
    ) -> None:
        """Emit health check failed event.

        Args:
            service_name: Service that failed check
            check_type: Type of check
            reason: Reason for failure
            duration_ms: Check duration in milliseconds
            details: Optional additional details
            trace_id: Optional trace ID
        """
        if not self.event_bus:
            return

        try:
            data = {
                "provider": self.__class__.__name__,
                "check_type": check_type,
                "reason": reason,
                "duration_ms": duration_ms,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            }
            if details:
                data["details"] = details

            await self.event_bus.publish_health_check_event(
                event_type=HealthCheckEvents.CHECK_FAILED,
                service_name=service_name,
                data=data,
                trace_id=trace_id,
            )
            logger.warning(f"Emitted health_check_failed for {service_name}: {reason}")
        except Exception as e:
            logger.warning(f"Failed to emit health_check_failed event: {e}")

    async def emit_deployment_rolled_back(
        self,
        deployment_id: str,
        service_name: str,
        from_version: str,
        to_version: str,
        reason: str,
        duration_seconds: float | None = None,
        trace_id: str | None = None,
    ) -> None:
        """Emit deployment rolled back event.

        Args:
            deployment_id: Deployment ID being rolled back
            service_name: Service being rolled back
            from_version: Version being rolled back from
            to_version: Version being rolled back to
            reason: Reason for rollback
            duration_seconds: Optional rollback duration
            trace_id: Optional trace ID
        """
        if not self.event_bus:
            return

        try:
            await self.event_bus.publish_deployment_event(
                event_type=DeploymentEvents.ROLLED_BACK,
                deployment_id=deployment_id,
                data={
                    "service": service_name,
                    "provider": self.__class__.__name__,
                    "from_version": from_version,
                    "to_version": to_version,
                    "reason": reason,
                    "duration_seconds": duration_seconds,
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                },
                trace_id=trace_id,
                region=self.region,
            )
            logger.info(f"Emitted deployment_rolled_back for {deployment_id}")
        except Exception as e:
            logger.warning(f"Failed to emit deployment_rolled_back event: {e}")

    async def emit_metrics(
        self,
        deployment_id: str,
        metrics: dict[str, Any],
        trace_id: str | None = None,
    ) -> None:
        """Emit deployment metrics event.

        Args:
            deployment_id: Deployment ID
            metrics: Metrics dict (error_rate, latency, cpu, memory, etc.)
            trace_id: Optional trace ID
        """
        if not self.event_bus:
            return

        try:
            await self.event_bus.publish_deployment_event(
                event_type=DeploymentEvents.METRICS_RECORDED,
                deployment_id=deployment_id,
                data={
                    "provider": self.__class__.__name__,
                    "metrics": metrics,
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                },
                trace_id=trace_id,
                region=self.region,
            )
        except Exception as e:
            logger.warning(f"Failed to emit metrics event: {e}")
