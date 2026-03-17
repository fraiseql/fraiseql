"""Retry configuration for FraiseQL clients."""

from __future__ import annotations

import random
from dataclasses import dataclass, field

from fraiseql.errors import NetworkError, TimeoutError


@dataclass
class RetryConfig:
    """Configuration for automatic request retries.

    Example::

        config = RetryConfig(max_attempts=3, base_delay=0.5, jitter=True)
        client = AsyncFraiseQLClient(url, retry=config)

    Attributes:
        max_attempts: Total number of attempts (1 means no retry).
        base_delay: Initial delay in seconds before the first retry.
        max_delay: Upper bound on the computed delay (seconds).
        jitter: Whether to add random noise to delays to avoid thundering herd.
        retry_on: Tuple of exception types that trigger a retry.
    """

    max_attempts: int = 1
    base_delay: float = 1.0
    max_delay: float = 30.0
    jitter: bool = True
    retry_on: tuple[type[Exception], ...] = field(
        default_factory=lambda: (NetworkError, TimeoutError),
    )

    def delay_for(self, attempt: int) -> float:
        """Return the delay (seconds) to wait before ``attempt`` (0-indexed).

        Uses exponential back-off: ``base_delay * 2 ** attempt``, capped at
        ``max_delay``.  When ``jitter`` is enabled, up to 10 % random noise is
        added.

        Args:
            attempt: Zero-based attempt index (0 → first retry delay).

        Returns:
            Delay in seconds.
        """
        delay = min(self.base_delay * (2**attempt), self.max_delay)
        if self.jitter:
            delay += random.uniform(0, delay * 0.1)
        return delay

    def should_retry(self, exc: BaseException) -> bool:
        """Return ``True`` if ``exc`` is in ``retry_on``.

        Args:
            exc: The exception to test.

        Returns:
            Whether the exception qualifies for a retry.
        """
        return isinstance(exc, self.retry_on)
