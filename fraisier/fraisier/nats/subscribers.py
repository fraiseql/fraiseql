"""Event subscribers and handler registry for NATS event bus.

Manages event subscriptions, handler registration, and event processing
with support for multiple handlers per event type, filtering, and lifecycle management.
"""

import asyncio
import logging
from typing import Any, Callable, Optional
from collections import defaultdict
from dataclasses import dataclass, field

from fraisier.logging import get_contextual_logger
from fraisier.nats.events import NatsEvent, DeploymentEvents, HealthCheckEvents, DatabaseEvents, MetricsEvents

logger = get_contextual_logger(__name__)


# Type aliases for handlers
EventHandler = Callable[[NatsEvent], Any]  # sync handler
AsyncEventHandler = Callable[[NatsEvent], Any]  # async handler


@dataclass
class EventFilter:
    """Filter for event matching."""

    event_type: Optional[str] = None  # e.g., "deployment.started"
    service: Optional[str] = None  # Filter by service name
    region: Optional[str] = None  # Filter by region
    deployment_id: Optional[str] = None  # Filter by deployment ID
    tags: dict[str, str] = field(default_factory=dict)  # Custom tag filters

    def matches(self, event: NatsEvent) -> bool:
        """Check if event matches this filter."""
        if self.event_type and event.event_type != self.event_type:
            return False

        if self.service and event.data.get("service") != self.service:
            return False

        if self.region and event.region != self.region:
            return False

        if self.deployment_id and event.data.get("deployment_id") != self.deployment_id:
            return False

        for key, value in self.tags.items():
            if event.data.get(key) != value:
                return False

        return True


@dataclass
class EventSubscription:
    """Represents a registered event subscription."""

    subscription_id: str
    handler: EventHandler | AsyncEventHandler
    filter: EventFilter
    is_async: bool
    retry_on_failure: bool = False
    retry_count: int = 3
    retry_delay: float = 1.0


class EventSubscriberRegistry:
    """Registry for event subscribers and handlers.

    Supports:
    - Multiple handlers per event type
    - Event filtering (by type, service, region, tags)
    - Async and sync handlers
    - Handler lifecycle management (register, unregister)
    - Event routing and processing
    - Error handling and retry logic
    """

    def __init__(self):
        """Initialize subscriber registry."""
        self.subscriptions: dict[str, EventSubscription] = {}
        self._subscription_counter = 0
        self._handlers_by_type: dict[str, list[str]] = defaultdict(list)
        self._lock = asyncio.Lock()

    def register(
        self,
        handler: EventHandler | AsyncEventHandler,
        event_filter: Optional[EventFilter] = None,
        is_async: bool = False,
        retry_on_failure: bool = False,
        retry_count: int = 3,
        retry_delay: float = 1.0,
    ) -> str:
        """Register an event handler.

        Args:
            handler: Event handler function (sync or async)
            event_filter: Optional filter for event matching
            is_async: Whether handler is async
            retry_on_failure: Whether to retry on failure
            retry_count: Number of retries
            retry_delay: Delay between retries in seconds

        Returns:
            Subscription ID for later unregistration
        """
        if event_filter is None:
            event_filter = EventFilter()

        self._subscription_counter += 1
        subscription_id = f"sub_{self._subscription_counter}"

        subscription = EventSubscription(
            subscription_id=subscription_id,
            handler=handler,
            filter=event_filter,
            is_async=is_async,
            retry_on_failure=retry_on_failure,
            retry_count=retry_count,
            retry_delay=retry_delay,
        )

        self.subscriptions[subscription_id] = subscription

        # Index by event type for faster lookup
        if event_filter.event_type:
            self._handlers_by_type[event_filter.event_type].append(subscription_id)

        logger.debug(f"Registered event handler {subscription_id}")
        return subscription_id

    def register_deployment_handler(
        self,
        handler: EventHandler | AsyncEventHandler,
        event_type: Optional[str] = None,
        service: Optional[str] = None,
        region: Optional[str] = None,
        is_async: bool = False,
    ) -> str:
        """Register a deployment event handler.

        Args:
            handler: Event handler function
            event_type: Specific deployment event type (e.g., "deployment.started")
            service: Filter by service name
            region: Filter by region
            is_async: Whether handler is async

        Returns:
            Subscription ID
        """
        filter = EventFilter(event_type=event_type, service=service, region=region)
        return self.register(handler, filter, is_async=is_async)

    def register_health_check_handler(
        self,
        handler: EventHandler | AsyncEventHandler,
        event_type: Optional[str] = None,
        service: Optional[str] = None,
        region: Optional[str] = None,
        is_async: bool = False,
    ) -> str:
        """Register a health check event handler.

        Args:
            handler: Event handler function
            event_type: Specific health check event type (e.g., "health_check.passed")
            service: Filter by service name
            region: Filter by region
            is_async: Whether handler is async

        Returns:
            Subscription ID
        """
        filter = EventFilter(event_type=event_type, service=service, region=region)
        return self.register(handler, filter, is_async=is_async)

    def register_database_handler(
        self,
        handler: EventHandler | AsyncEventHandler,
        event_type: Optional[str] = None,
        region: Optional[str] = None,
        is_async: bool = False,
    ) -> str:
        """Register a database event handler.

        Args:
            handler: Event handler function
            event_type: Specific database event type
            region: Filter by region
            is_async: Whether handler is async

        Returns:
            Subscription ID
        """
        filter = EventFilter(event_type=event_type, region=region)
        return self.register(handler, filter, is_async=is_async)

    def unregister(self, subscription_id: str) -> bool:
        """Unregister an event handler.

        Args:
            subscription_id: ID returned from register()

        Returns:
            True if unregistered, False if not found
        """
        if subscription_id not in self.subscriptions:
            return False

        subscription = self.subscriptions.pop(subscription_id)

        # Remove from type index
        if subscription.filter.event_type:
            self._handlers_by_type[subscription.filter.event_type].remove(subscription_id)

        logger.debug(f"Unregistered event handler {subscription_id}")
        return True

    async def handle_event(self, event: NatsEvent) -> None:
        """Process an event through all matching handlers.

        Args:
            event: Event to process
        """
        # Find matching subscriptions
        matching_subscriptions = [
            sub for sub in self.subscriptions.values()
            if sub.filter.matches(event)
        ]

        if not matching_subscriptions:
            logger.debug(f"No handlers for event {event.event_type}")
            return

        # Process all matching handlers
        tasks = []
        for subscription in matching_subscriptions:
            if subscription.is_async:
                tasks.append(
                    self._handle_async(subscription, event)
                )
            else:
                await self._handle_sync(subscription, event)

        # Wait for all async handlers
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)

    async def _handle_sync(
        self,
        subscription: EventSubscription,
        event: NatsEvent,
    ) -> None:
        """Handle sync handler with retry logic.

        Args:
            subscription: Event subscription
            event: Event to handle
        """
        attempt = 0
        while attempt < subscription.retry_count:
            try:
                result = subscription.handler(event)
                logger.debug(
                    f"Event handler {subscription.subscription_id} "
                    f"processed {event.event_type}"
                )
                return

            except Exception as e:
                attempt += 1
                if attempt >= subscription.retry_count:
                    logger.error(
                        f"Event handler {subscription.subscription_id} failed "
                        f"after {subscription.retry_count} attempts: {e}",
                        exc_info=True,
                    )
                    return

                if subscription.retry_on_failure:
                    await asyncio.sleep(subscription.retry_delay)
                    logger.warning(
                        f"Event handler {subscription.subscription_id} "
                        f"retrying after failure: {e}"
                    )
                else:
                    return

    async def _handle_async(
        self,
        subscription: EventSubscription,
        event: NatsEvent,
    ) -> None:
        """Handle async handler with retry logic.

        Args:
            subscription: Event subscription
            event: Event to handle
        """
        attempt = 0
        while attempt < subscription.retry_count:
            try:
                result = await subscription.handler(event)
                logger.debug(
                    f"Async event handler {subscription.subscription_id} "
                    f"processed {event.event_type}"
                )
                return

            except Exception as e:
                attempt += 1
                if attempt >= subscription.retry_count:
                    logger.error(
                        f"Async event handler {subscription.subscription_id} failed "
                        f"after {subscription.retry_count} attempts: {e}",
                        exc_info=True,
                    )
                    return

                if subscription.retry_on_failure:
                    await asyncio.sleep(subscription.retry_delay)
                    logger.warning(
                        f"Async event handler {subscription.subscription_id} "
                        f"retrying after failure: {e}"
                    )
                else:
                    return

    def get_subscription_count(self) -> int:
        """Get total number of registered subscriptions.

        Returns:
            Number of subscriptions
        """
        return len(self.subscriptions)

    def get_subscriptions_for_event_type(self, event_type: str) -> list[EventSubscription]:
        """Get all subscriptions for a specific event type.

        Args:
            event_type: Event type

        Returns:
            List of subscriptions
        """
        subscription_ids = self._handlers_by_type.get(event_type, [])
        return [self.subscriptions[sid] for sid in subscription_ids if sid in self.subscriptions]

    def clear_all(self) -> int:
        """Clear all registered subscriptions.

        Returns:
            Number of subscriptions cleared
        """
        count = len(self.subscriptions)
        self.subscriptions.clear()
        self._handlers_by_type.clear()
        self._subscription_counter = 0
        logger.debug(f"Cleared {count} event subscriptions")
        return count


class EventHandlers:
    """Pre-built event handlers for common deployment operations."""

    @staticmethod
    def log_event(event: NatsEvent) -> None:
        """Log event details.

        Args:
            event: Event to log
        """
        logger.info(
            f"Event: {event.event_type} | "
            f"Service: {event.data.get('service', 'unknown')} | "
            f"Status: {event.data.get('status', 'N/A')}"
        )

    @staticmethod
    async def log_event_async(event: NatsEvent) -> None:
        """Log event details (async version).

        Args:
            event: Event to log
        """
        logger.info(
            f"Event (async): {event.event_type} | "
            f"Service: {event.data.get('service', 'unknown')} | "
            f"Status: {event.data.get('status', 'N/A')}"
        )

    @staticmethod
    def count_events(event: NatsEvent) -> None:
        """Count events by type (for metrics).

        Args:
            event: Event to count
        """
        # This is a placeholder - in production, would update metrics
        logger.debug(f"Event count metric: {event.event_type}")

    @staticmethod
    async def notify_on_failure(event: NatsEvent) -> None:
        """Send notification on deployment failure.

        Args:
            event: Failure event
        """
        if event.data.get("status") == "failure" or "failed" in event.event_type:
            service = event.data.get("service", "unknown")
            reason = event.data.get("error", "unknown error")
            logger.warning(
                f"Deployment failure notification: "
                f"Service={service}, Reason={reason}"
            )

    @staticmethod
    def trigger_webhook(webhook_url: str) -> EventHandler:
        """Create a webhook handler.

        Args:
            webhook_url: URL to POST event to

        Returns:
            Event handler function
        """
        def handler(event: NatsEvent) -> None:
            # This is a placeholder - would implement actual HTTP POST
            logger.debug(f"Would trigger webhook: {webhook_url} with event {event.event_type}")

        return handler

    @staticmethod
    def create_metric_recorder(metric_name: str) -> EventHandler:
        """Create a metric recording handler.

        Args:
            metric_name: Metric name to record

        Returns:
            Event handler function
        """
        def handler(event: NatsEvent) -> None:
            # This is a placeholder - would record actual metrics
            logger.debug(
                f"Recording metric {metric_name} for event {event.event_type}"
            )

        return handler


# Global registry instance
_global_registry: Optional[EventSubscriberRegistry] = None


def get_subscriber_registry() -> EventSubscriberRegistry:
    """Get the global event subscriber registry.

    Returns:
        Event subscriber registry instance
    """
    global _global_registry
    if _global_registry is None:
        _global_registry = EventSubscriberRegistry()
    return _global_registry


def reset_subscriber_registry() -> None:
    """Reset the global subscriber registry (for testing)."""
    global _global_registry
    _global_registry = None
