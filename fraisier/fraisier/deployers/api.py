"""API fraise deployer - for web services and APIs."""

import logging
import subprocess
import time
from typing import Any

import requests

from .base import BaseDeployer, DeploymentResult, DeploymentStatus

logger = logging.getLogger(__name__)


class APIDeployer(BaseDeployer):
    """Deployer for API/web service fraises.

    Handles:
    - Git pull from repository
    - Database migrations
    - Service restart via systemd
    - Health check verification
    """

    def __init__(self, config: dict[str, Any]):
        super().__init__(config)
        self.git_repo = config.get("git_repo")
        self.app_path = config.get("app_path")
        self.systemd_service = config.get("systemd_service")
        self.health_check_url = config.get("health_check", {}).get("url")
        self.health_check_timeout = config.get("health_check", {}).get("timeout", 30)
        self.database_config = config.get("database", {})

    def get_current_version(self) -> str | None:
        """Get currently deployed git commit."""
        if not self.app_path:
            return None

        try:
            result = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=self.app_path,
                capture_output=True,
                text=True,
                check=True,
            )
            return result.stdout.strip()[:8]
        except subprocess.CalledProcessError:
            return None

    def get_latest_version(self) -> str | None:
        """Get latest git commit from remote."""
        if not self.git_repo:
            return None

        try:
            result = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=self.git_repo,
                capture_output=True,
                text=True,
                check=True,
            )
            return result.stdout.strip()[:8]
        except subprocess.CalledProcessError:
            return None

    def execute(self) -> DeploymentResult:
        """Execute API deployment."""
        start_time = time.time()
        old_version = self.get_current_version()

        try:
            # Step 1: Pull latest code
            logger.info(f"Pulling latest code to {self.app_path}")
            self._git_pull()

            # Step 2: Run database migrations if configured
            if self.database_config:
                logger.info("Running database migrations")
                self._run_migrations()

            # Step 3: Restart service
            if self.systemd_service:
                logger.info(f"Restarting service: {self.systemd_service}")
                self._restart_service()

            # Step 4: Health check
            if self.health_check_url:
                logger.info(f"Running health check: {self.health_check_url}")
                if not self._wait_for_health():
                    raise RuntimeError("Health check failed after deployment")

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
            logger.exception(f"Deployment failed: {e}")

            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                old_version=old_version,
                duration_seconds=duration,
                error_message=str(e),
            )

    def _git_pull(self) -> None:
        """Pull latest code from git."""
        if not self.app_path:
            raise ValueError("app_path not configured")

        subprocess.run(
            ["git", "pull", "--ff-only"],
            cwd=self.app_path,
            check=True,
            capture_output=True,
        )

    def _run_migrations(self) -> None:
        """Run database migrations."""
        strategy = self.database_config.get("strategy", "apply")

        if strategy == "rebuild":
            # Full database rebuild (dev/staging only!)
            logger.warning("Running database rebuild - this drops all data!")
            # Implementation depends on your database setup
            pass
        elif strategy == "apply":
            # Safe migrations only
            logger.info("Applying database migrations")
            # Implementation depends on your migration tool
            pass

    def _restart_service(self) -> None:
        """Restart systemd service."""
        if not self.systemd_service:
            return

        subprocess.run(
            ["sudo", "systemctl", "restart", self.systemd_service],
            check=True,
            capture_output=True,
        )

    def _wait_for_health(self, max_attempts: int = 10, delay: float = 3.0) -> bool:
        """Wait for health check to pass."""
        if not self.health_check_url:
            return True

        for attempt in range(max_attempts):
            try:
                response = requests.get(
                    self.health_check_url,
                    timeout=self.health_check_timeout,
                )
                if response.status_code == 200:
                    logger.info(f"Health check passed on attempt {attempt + 1}")
                    return True
            except requests.RequestException as e:
                logger.warning(f"Health check attempt {attempt + 1} failed: {e}")

            if attempt < max_attempts - 1:
                time.sleep(delay)

        return False

    def health_check(self) -> bool:
        """Check if API is healthy."""
        return self._wait_for_health(max_attempts=1)

    def rollback(self, to_version: str | None = None) -> DeploymentResult:
        """Rollback to previous version."""
        start_time = time.time()
        current_version = self.get_current_version()

        try:
            if to_version:
                # Rollback to specific version
                subprocess.run(
                    ["git", "checkout", to_version],
                    cwd=self.app_path,
                    check=True,
                    capture_output=True,
                )
            else:
                # Rollback to previous commit
                subprocess.run(
                    ["git", "checkout", "HEAD~1"],
                    cwd=self.app_path,
                    check=True,
                    capture_output=True,
                )

            # Restart service
            if self.systemd_service:
                self._restart_service()

            # Wait for health
            if self.health_check_url:
                self._wait_for_health()

            new_version = self.get_current_version()
            duration = time.time() - start_time

            return DeploymentResult(
                success=True,
                status=DeploymentStatus.ROLLED_BACK,
                old_version=current_version,
                new_version=new_version,
                duration_seconds=duration,
            )

        except Exception as e:
            duration = time.time() - start_time
            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                old_version=current_version,
                duration_seconds=duration,
                error_message=f"Rollback failed: {e}",
            )
