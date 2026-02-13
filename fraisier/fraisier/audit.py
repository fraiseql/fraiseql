"""Audit logging for compliance and debugging.

Logs all significant actions and state changes for:
- Compliance and audit trails
- Debugging and troubleshooting
- Security event tracking
- Deployment history
"""

from typing import Any

from .logging import ContextualLogger


class AuditLogger:
    """Log all significant actions for compliance and debugging.

    Usage:
        audit = AuditLogger(contextual_logger)
        audit.log_deployment_start("deploy-123", "api", "production", "bare_metal")
        audit.log_deployment_complete("deploy-123", "success", 45.2)
    """

    def __init__(self, logger: ContextualLogger):
        """Initialize audit logger.

        Args:
            logger: ContextualLogger instance for output
        """
        self.logger = logger

    def _redact_value(self, value: Any, key: str) -> Any:
        """Redact sensitive values.

        Args:
            value: Value to potentially redact
            key: Key name

        Returns:
            Redacted or original value
        """
        sensitive_keys = {"password", "api_key", "secret", "token", "auth"}
        if any(sensitive in key.lower() for sensitive in sensitive_keys):
            return "***REDACTED***"
        return value

    def log_deployment_start(
        self,
        deployment_id: str,
        fraise: str,
        environment: str,
        provider: str,
        version: str | None = None,
    ) -> None:
        """Log deployment start event.

        Args:
            deployment_id: Unique deployment ID
            fraise: Fraise name
            environment: Target environment
            provider: Provider being used
            version: Version being deployed
        """
        self.logger.info(
            "Deployment started",
            event_type="deployment_start",
            deployment_id=deployment_id,
            fraise=fraise,
            environment=environment,
            provider=provider,
            version=version,
        )

    def log_deployment_complete(
        self,
        deployment_id: str,
        status: str,
        duration: float,
        error: str | None = None,
    ) -> None:
        """Log deployment completion event.

        Args:
            deployment_id: Deployment ID
            status: Status (success, failed)
            duration: Deployment duration in seconds
            error: Error message if failed
        """
        self.logger.info(
            f"Deployment {status}",
            event_type="deployment_complete",
            deployment_id=deployment_id,
            status=status,
            duration=duration,
            error=error,
        )

    def log_deployment_rollback(
        self,
        deployment_id: str,
        reason: str,
        from_version: str | None,
        to_version: str | None,
        duration: float,
    ) -> None:
        """Log deployment rollback event.

        Args:
            deployment_id: Deployment ID
            reason: Reason for rollback
            from_version: Current version being rolled back from
            to_version: Version being rolled back to
            duration: Rollback duration in seconds
        """
        self.logger.info(
            "Deployment rolled back",
            event_type="deployment_rollback",
            deployment_id=deployment_id,
            reason=reason,
            from_version=from_version,
            to_version=to_version,
            duration=duration,
        )

    def log_health_check(
        self,
        deployment_id: str,
        check_type: str,
        status: str,
        duration: float,
        message: str | None = None,
    ) -> None:
        """Log health check result.

        Args:
            deployment_id: Deployment ID
            check_type: Type of check (http, tcp, exec, api_status)
            status: Result (pass, fail)
            duration: Check duration in seconds
            message: Additional message
        """
        self.logger.info(
            f"Health check {status}",
            event_type="health_check",
            deployment_id=deployment_id,
            check_type=check_type,
            status=status,
            duration=duration,
            message=message,
        )

    def log_configuration_change(
        self,
        config_type: str,
        changed_fields: dict[str, tuple[Any, Any]],
        changed_by: str | None = None,
        reason: str | None = None,
    ) -> None:
        """Log configuration change event.

        Args:
            config_type: Type of config (provider_config, deployment_config, etc.)
            changed_fields: Dict of field -> (old_value, new_value)
            changed_by: User or system that made the change
            reason: Reason for change
        """
        # Redact sensitive values
        redacted_fields = {}
        for field, (old_val, new_val) in changed_fields.items():
            redacted_fields[field] = (
                self._redact_value(old_val, field),
                self._redact_value(new_val, field),
            )

        self.logger.info(
            f"{config_type} changed",
            event_type="configuration_change",
            config_type=config_type,
            changed_fields=redacted_fields,
            changed_by=changed_by,
            reason=reason,
        )

    def log_provider_error(
        self,
        deployment_id: str,
        provider: str,
        error_type: str,
        error_message: str,
        recoverable: bool = False,
    ) -> None:
        """Log provider error event.

        Args:
            deployment_id: Deployment ID
            provider: Provider name
            error_type: Error type (connection, timeout, auth, etc.)
            error_message: Error message
            recoverable: Whether error is recoverable
        """
        self.logger.warning(
            f"Provider error: {error_type}",
            event_type="provider_error",
            deployment_id=deployment_id,
            provider=provider,
            error_type=error_type,
            error_message=error_message,
            recoverable=recoverable,
        )

    def log_lock_acquired(
        self,
        service: str,
        provider: str,
        deployment_id: str | None = None,
        wait_time: float = 0,
    ) -> None:
        """Log deployment lock acquired event.

        Args:
            service: Service name
            provider: Provider name
            deployment_id: Deployment ID if applicable
            wait_time: Time waited for lock in seconds
        """
        self.logger.info(
            "Deployment lock acquired",
            event_type="lock_acquired",
            service=service,
            provider=provider,
            deployment_id=deployment_id,
            wait_time=wait_time,
        )

    def log_lock_timeout(
        self,
        service: str,
        provider: str,
        timeout: float,
    ) -> None:
        """Log deployment lock timeout event.

        Args:
            service: Service name
            provider: Provider name
            timeout: Lock timeout in seconds
        """
        self.logger.warning(
            "Deployment lock acquisition timed out",
            event_type="lock_timeout",
            service=service,
            provider=provider,
            timeout=timeout,
        )

    def log_webhook_received(
        self,
        event_type: str,
        repository: str,
        branch: str,
        commit_sha: str,
    ) -> None:
        """Log webhook received event.

        Args:
            event_type: Webhook event type (push, pull_request, etc.)
            repository: Repository name
            branch: Git branch
            commit_sha: Commit SHA
        """
        self.logger.info(
            f"Webhook received: {event_type}",
            event_type="webhook_received",
            webhook_type=event_type,
            repository=repository,
            branch=branch,
            commit_sha=commit_sha,
        )

    def log_webhook_deployment(
        self,
        webhook_id: str,
        deployment_id: str,
        fraise: str,
        environment: str,
    ) -> None:
        """Log webhook-triggered deployment.

        Args:
            webhook_id: Webhook ID
            deployment_id: Deployment ID
            fraise: Fraise name
            environment: Environment
        """
        self.logger.info(
            "Webhook triggered deployment",
            event_type="webhook_deployment",
            webhook_id=webhook_id,
            deployment_id=deployment_id,
            fraise=fraise,
            environment=environment,
        )

    def log_security_event(
        self,
        event_type: str,
        severity: str,
        details: dict[str, Any],
    ) -> None:
        """Log security-related event.

        Args:
            event_type: Event type (auth_failure, unauthorized_access, etc.)
            severity: Severity level (low, medium, high, critical)
            details: Additional event details
        """
        if severity == "critical":
            log_method = self.logger.error
        elif severity in ("medium", "high"):
            log_method = self.logger.warning
        else:
            log_method = self.logger.info

        log_method(
            f"Security event: {event_type}",
            event_type="security_event",
            security_event_type=event_type,
            severity=severity,
            details=details,
        )

    def log_performance_event(
        self,
        operation: str,
        duration: float,
        threshold: float,
        status: str = "slow",
    ) -> None:
        """Log performance event when operation exceeds threshold.

        Args:
            operation: Operation name
            duration: Actual duration in seconds
            threshold: Threshold duration in seconds
            status: Status (slow, critical_slow, etc.)
        """
        self.logger.warning(
            f"Performance issue: {operation} took {duration}s (threshold: {threshold}s)",
            event_type="performance_event",
            operation=operation,
            duration=duration,
            threshold=threshold,
            status=status,
        )

    def log_system_event(
        self,
        event_type: str,
        message: str,
        severity: str = "info",
        details: dict[str, Any] | None = None,
    ) -> None:
        """Log system-level event.

        Args:
            event_type: Event type (startup, shutdown, config_reload, etc.)
            message: Event message
            severity: Severity level (info, warning, error)
            details: Additional details
        """
        log_method = {
            "info": self.logger.info,
            "warning": self.logger.warning,
            "error": self.logger.error,
        }.get(severity, self.logger.info)

        log_method(
            message,
            event_type="system_event",
            system_event_type=event_type,
            severity=severity,
            details=details or {},
        )
