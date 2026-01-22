"""Health check management with retries, monitoring, and advanced checks.

Provides enhanced health checking capabilities:
- Retry with exponential backoff
- Timeout enforcement
- Metrics recording
- Multiple check types (HTTP, TCP, exec)
- Health status tracking
"""

import logging
import subprocess
import time
from abc import ABC, abstractmethod
from typing import Any

from .logging import ContextualLogger
from .metrics import get_metrics_recorder


class HealthCheckResult:
    """Result from a health check operation.

    Attributes:
        success: Whether health check passed
        check_type: Type of check performed
        duration: Check duration in seconds
        message: Additional details or error message
        timestamp: When check was performed
    """

    def __init__(
        self,
        success: bool,
        check_type: str,
        duration: float,
        message: str | None = None,
    ):
        """Initialize health check result.

        Args:
            success: Whether check passed
            check_type: Type of check (http, tcp, exec, status_api, etc.)
            duration: Check duration in seconds
            message: Details or error message
        """
        self.success = success
        self.check_type = check_type
        self.duration = duration
        self.message = message
        self.timestamp = time.time()

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for logging/serialization."""
        return {
            "success": self.success,
            "check_type": self.check_type,
            "duration": self.duration,
            "message": self.message,
            "timestamp": self.timestamp,
        }


class HealthChecker(ABC):
    """Base class for health check implementations."""

    check_type: str

    @abstractmethod
    def check(self, timeout: float = 5.0) -> HealthCheckResult:
        """Perform health check.

        Args:
            timeout: Maximum time to wait for check in seconds

        Returns:
            HealthCheckResult with status and details
        """
        pass


class HTTPHealthChecker(HealthChecker):
    """HTTP-based health check."""

    check_type = "http"

    def __init__(self, url: str):
        """Initialize HTTP health checker.

        Args:
            url: URL to check (e.g., 'http://localhost:8000/health')
        """
        self.url = url
        self.logger = logging.getLogger(__name__)

    def check(self, timeout: float = 5.0) -> HealthCheckResult:
        """Check HTTP endpoint.

        Args:
            timeout: Request timeout in seconds

        Returns:
            HealthCheckResult with HTTP status
        """
        import urllib.error
        import urllib.request

        start = time.time()
        try:
            response = urllib.request.urlopen(self.url, timeout=timeout)
            duration = time.time() - start
            return HealthCheckResult(
                success=response.status < 400,
                check_type=self.check_type,
                duration=duration,
                message=f"HTTP {response.status}",
            )
        except urllib.error.HTTPError as e:
            duration = time.time() - start
            return HealthCheckResult(
                success=False,
                check_type=self.check_type,
                duration=duration,
                message=f"HTTP {e.code}: {e.reason}",
            )
        except Exception as e:
            duration = time.time() - start
            return HealthCheckResult(
                success=False,
                check_type=self.check_type,
                duration=duration,
                message=f"Connection error: {e}",
            )


class TCPHealthChecker(HealthChecker):
    """TCP port-based health check."""

    check_type = "tcp"

    def __init__(self, host: str, port: int):
        """Initialize TCP health checker.

        Args:
            host: Host to connect to
            port: Port to check
        """
        self.host = host
        self.port = port
        self.logger = logging.getLogger(__name__)

    def check(self, timeout: float = 5.0) -> HealthCheckResult:
        """Check TCP port connectivity.

        Args:
            timeout: Connection timeout in seconds

        Returns:
            HealthCheckResult with TCP connectivity status
        """
        import socket

        start = time.time()
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(timeout)
            result = sock.connect_ex((self.host, self.port))
            sock.close()

            duration = time.time() - start
            success = result == 0

            return HealthCheckResult(
                success=success,
                check_type=self.check_type,
                duration=duration,
                message="TCP connection successful" if success else f"TCP error: {result}",
            )
        except Exception as e:
            duration = time.time() - start
            return HealthCheckResult(
                success=False,
                check_type=self.check_type,
                duration=duration,
                message=f"TCP check error: {e}",
            )


class ExecHealthChecker(HealthChecker):
    """Command execution-based health check."""

    check_type = "exec"

    def __init__(self, command: str):
        """Initialize exec health checker.

        Args:
            command: Command to execute (should return 0 on success)
        """
        self.command = command
        self.logger = logging.getLogger(__name__)

    def check(self, timeout: float = 5.0) -> HealthCheckResult:
        """Execute health check command.

        Args:
            timeout: Command execution timeout in seconds

        Returns:
            HealthCheckResult with command exit status
        """
        start = time.time()
        try:
            result = subprocess.run(
                self.command,
                shell=True,
                timeout=timeout,
                capture_output=True,
                text=True,
            )
            duration = time.time() - start

            return HealthCheckResult(
                success=result.returncode == 0,
                check_type=self.check_type,
                duration=duration,
                message=(
                    result.stdout.strip() or f"Exit code: {result.returncode}"
                ),
            )
        except subprocess.TimeoutExpired:
            duration = time.time() - start
            return HealthCheckResult(
                success=False,
                check_type=self.check_type,
                duration=duration,
                message=f"Command timeout after {timeout}s",
            )
        except Exception as e:
            duration = time.time() - start
            return HealthCheckResult(
                success=False,
                check_type=self.check_type,
                duration=duration,
                message=f"Execution error: {e}",
            )


class HealthCheckManager:
    """Manage health checks with retries, timeouts, and monitoring.

    Usage:
        manager = HealthCheckManager(
            provider="bare_metal",
            deployment_id="deploy-123"
        )
        result = manager.check_with_retries(
            HTTPHealthChecker("http://localhost:8000/health"),
            max_retries=3
        )
        if result.success:
            print("Service is healthy")
    """

    def __init__(self, provider: str | None = None, deployment_id: str | None = None):
        """Initialize health check manager.

        Args:
            provider: Provider name for metrics
            deployment_id: Deployment ID for logging and metrics
        """
        self.provider = provider
        self.deployment_id = deployment_id
        self.logger = ContextualLogger(__name__)
        self.metrics = get_metrics_recorder()

    def check_with_retries(
        self,
        checker: HealthChecker,
        max_retries: int = 3,
        initial_delay: float = 1.0,
        backoff_factor: float = 2.0,
        max_delay: float = 30.0,
        timeout: float = 5.0,
    ) -> HealthCheckResult:
        """Perform health check with exponential backoff retries.

        Args:
            checker: HealthChecker instance
            max_retries: Maximum number of retry attempts
            initial_delay: Initial delay between retries in seconds
            backoff_factor: Multiplier for exponential backoff
            max_delay: Maximum delay between retries
            timeout: Timeout per check attempt

        Returns:
            HealthCheckResult from last attempt
        """
        last_result = None
        delay = initial_delay

        for attempt in range(max_retries):
            try:
                with self.logger.context(
                    attempt=attempt + 1,
                    deployment_id=self.deployment_id,
                    check_type=checker.check_type,
                ):
                    last_result = checker.check(timeout=timeout)

                    if last_result.success:
                        self.logger.info(
                            f"Health check passed on attempt {attempt + 1}",
                            duration=last_result.duration,
                        )
                        return last_result

                    self.logger.warning(
                        f"Health check failed on attempt {attempt + 1}",
                        message=last_result.message,
                        duration=last_result.duration,
                    )

            except Exception as e:
                self.logger.error(
                    f"Error during health check attempt {attempt + 1}: {e}",
                    _exc_info=True,
                )

            # Wait before retry (except on last attempt)
            if attempt < max_retries - 1:
                wait_time = min(delay, max_delay)
                self.logger.info(f"Retrying in {wait_time:.1f}s")
                time.sleep(wait_time)
                delay *= backoff_factor

        # All retries exhausted
        if last_result is None:
            last_result = HealthCheckResult(
                success=False,
                check_type=checker.check_type,
                duration=0,
                message=f"All {max_retries} health check attempts failed",
            )

        self.logger.error(
            f"Health check failed after {max_retries} attempts",
            message=last_result.message,
        )
        return last_result

    def check_and_record_metrics(
        self,
        checker: HealthChecker,
        max_retries: int = 3,
        timeout: float = 5.0,
    ) -> HealthCheckResult:
        """Perform health check and record metrics.

        Args:
            checker: HealthChecker instance
            max_retries: Maximum retry attempts
            timeout: Timeout per check

        Returns:
            HealthCheckResult with metrics recorded
        """
        result = self.check_with_retries(
            checker,
            max_retries=max_retries,
            timeout=timeout,
        )

        # Record metrics
        if self.provider:
            status = "pass" if result.success else "fail"
            self.metrics.record_health_check(
                provider=self.provider,
                check_type=result.check_type,
                status=status,
                duration=result.duration,
            )

        return result

    def check_service_ready(
        self,
        checker: HealthChecker,
        max_retries: int = 5,
        initial_delay: float = 2.0,
        timeout: float = 5.0,
    ) -> tuple[bool, str]:
        """Wait for service to be ready with retries.

        Useful during deployment startup when service needs time to initialize.

        Args:
            checker: HealthChecker instance
            max_retries: Maximum wait attempts
            initial_delay: Initial delay between checks
            timeout: Timeout per check

        Returns:
            Tuple of (success: bool, message: str)
        """
        result = self.check_with_retries(
            checker,
            max_retries=max_retries,
            initial_delay=initial_delay,
            timeout=timeout,
        )

        return result.success, result.message or "Health check failed"

    def check_service_healthy(
        self,
        checker: HealthChecker,
        timeout: float = 5.0,
    ) -> bool:
        """Quick health check without retries.

        Args:
            checker: HealthChecker instance
            timeout: Timeout for check

        Returns:
            True if service is healthy, False otherwise
        """
        result = self.check_with_retries(
            checker,
            max_retries=1,
            timeout=timeout,
        )
        return result.success


class CompositeHealthChecker:
    """Perform multiple health checks and aggregate results.

    Usage:
        composite = CompositeHealthChecker()
        composite.add_check("http", HTTPHealthChecker("http://localhost/health"))
        composite.add_check("tcp", TCPHealthChecker("localhost", 8000))
        success, results = composite.check_all(require_all=True)
    """

    def __init__(self):
        """Initialize composite health checker."""
        self.checks: dict[str, HealthChecker] = {}
        self.logger = logging.getLogger(__name__)

    def add_check(self, name: str, checker: HealthChecker) -> None:
        """Add a health check.

        Args:
            name: Name for this check
            checker: HealthChecker instance
        """
        self.checks[name] = checker

    def check_all(
        self,
        require_all: bool = True,
        timeout: float = 5.0,
    ) -> tuple[bool, dict[str, HealthCheckResult]]:
        """Execute all health checks.

        Args:
            require_all: If True, all checks must pass. If False, at least one must pass.
            timeout: Timeout per check

        Returns:
            Tuple of (overall_success: bool, results: dict of check results)
        """
        results = {}
        passed = 0
        failed = 0

        for name, checker in self.checks.items():
            try:
                result = checker.check(timeout=timeout)
                results[name] = result

                if result.success:
                    passed += 1
                    self.logger.info(f"Check '{name}' passed")
                else:
                    failed += 1
                    self.logger.warning(
                        f"Check '{name}' failed: {result.message}"
                    )
            except Exception as e:
                result = HealthCheckResult(
                    success=False,
                    check_type=checker.check_type,
                    duration=0,
                    message=f"Error: {e}",
                )
                results[name] = result
                failed += 1
                self.logger.error(f"Error in check '{name}': {e}")

        # Determine overall success
        if require_all:
            overall_success = failed == 0
        else:
            overall_success = passed > 0

        return overall_success, results
