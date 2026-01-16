"""Scheduled fraise deployer - for cron jobs and timers."""

import logging
import subprocess
import time
from typing import Any

from .base import BaseDeployer, DeploymentResult, DeploymentStatus

logger = logging.getLogger(__name__)


class ScheduledDeployer(BaseDeployer):
    """Deployer for scheduled/cron job fraises.

    Handles systemd timers and one-shot services.
    """

    def __init__(self, config: dict[str, Any]):
        super().__init__(config)
        self.systemd_service = config.get("systemd_service")
        self.systemd_timer = config.get("systemd_timer")
        self.script_path = config.get("script_path")
        self.job_name = config.get("job_name")

    def get_current_version(self) -> str | None:
        """Get timer/service status as version proxy."""
        if not self.systemd_timer:
            return None

        try:
            result = subprocess.run(
                ["systemctl", "show", self.systemd_timer, "--property=ActiveState"],
                capture_output=True,
                text=True,
                check=True,
            )
            state = result.stdout.strip().split("=")[1]
            return f"timer:{state}"
        except subprocess.CalledProcessError:
            return None

    def get_latest_version(self) -> str | None:
        """For scheduled jobs, latest = current (always up to date)."""
        return self.get_current_version()

    def is_deployment_needed(self) -> bool:
        """Check if timer needs to be enabled/restarted."""
        if not self.systemd_timer:
            return False

        try:
            result = subprocess.run(
                ["systemctl", "is-active", self.systemd_timer],
                capture_output=True,
                text=True,
            )
            return result.returncode != 0
        except subprocess.CalledProcessError:
            return True

    def execute(self) -> DeploymentResult:
        """Execute scheduled job deployment.

        Ensures the timer is enabled and running.
        """
        start_time = time.time()
        old_version = self.get_current_version()

        try:
            if self.systemd_timer:
                logger.info(f"Enabling timer: {self.systemd_timer}")

                # Enable timer
                subprocess.run(
                    ["sudo", "systemctl", "enable", self.systemd_timer],
                    check=True,
                    capture_output=True,
                )

                # Start timer
                subprocess.run(
                    ["sudo", "systemctl", "start", self.systemd_timer],
                    check=True,
                    capture_output=True,
                )

                # Reload daemon
                subprocess.run(
                    ["sudo", "systemctl", "daemon-reload"],
                    check=True,
                    capture_output=True,
                )

            new_version = self.get_current_version()
            duration = time.time() - start_time

            return DeploymentResult(
                success=True,
                status=DeploymentStatus.SUCCESS,
                old_version=old_version,
                new_version=new_version,
                duration_seconds=duration,
            )

        except Exception as e:
            duration = time.time() - start_time
            logger.exception(f"Scheduled job deployment failed: {e}")

            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                old_version=old_version,
                duration_seconds=duration,
                error_message=str(e),
            )

    def health_check(self) -> bool:
        """Check if timer is active."""
        if not self.systemd_timer:
            return True

        try:
            result = subprocess.run(
                ["systemctl", "is-active", self.systemd_timer],
                capture_output=True,
                text=True,
            )
            return result.returncode == 0
        except subprocess.CalledProcessError:
            return False
