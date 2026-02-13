"""ETL fraise deployer - for data pipeline jobs."""

import logging
import subprocess
import time
from typing import Any

from .base import BaseDeployer, DeploymentResult, DeploymentStatus

logger = logging.getLogger(__name__)


class ETLDeployer(BaseDeployer):
    """Deployer for ETL/data pipeline fraises.

    ETL fraises typically share code with API fraises but run
    as separate scheduled or triggered jobs.
    """

    def __init__(self, config: dict[str, Any]):
        super().__init__(config)
        self.app_path = config.get("app_path")
        self.script_path = config.get("script_path")
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
        """Get latest git commit (ETL shares code with API)."""
        return self.get_current_version()

    def execute(self) -> DeploymentResult:
        """Execute ETL deployment.

        For ETL fraises, deployment typically means:
        1. Verify the shared code is up to date
        2. Run any ETL-specific migrations
        3. Optionally trigger a test run
        """
        start_time = time.time()
        old_version = self.get_current_version()

        try:
            # ETL typically shares code with API, so we just verify
            logger.info(f"Verifying ETL deployment at {self.app_path}")

            # Verify script exists
            if self.script_path:
                full_path = f"{self.app_path}/{self.script_path}"
                result = subprocess.run(
                    ["test", "-f", full_path],
                    capture_output=True,
                )
                if result.returncode != 0:
                    raise FileNotFoundError(f"ETL script not found: {full_path}")

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
            logger.exception(f"ETL deployment verification failed: {e}")

            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                old_version=old_version,
                duration_seconds=duration,
                error_message=str(e),
            )

    def rollback(self, to_version: str | None = None) -> DeploymentResult:
        """Rollback ETL deployment to previous version.

        For ETL fraises, rollback typically means restoring the shared code
        to the previous git commit (similar to API deployer).

        Args:
            to_version: Specific commit hash to rollback to, or previous if None

        Returns:
            DeploymentResult with rollback status
        """
        start_time = time.time()
        current_version = self.get_current_version()

        try:
            if not self.app_path:
                raise ValueError("app_path not configured for rollback")

            if to_version:
                # Rollback to specific version
                logger.info(f"Rolling back to commit: {to_version}")
                subprocess.run(
                    ["git", "checkout", to_version],
                    cwd=self.app_path,
                    check=True,
                    capture_output=True,
                )
            else:
                # Rollback to previous commit
                logger.info("Rolling back to previous commit")
                subprocess.run(
                    ["git", "checkout", "HEAD~1"],
                    cwd=self.app_path,
                    check=True,
                    capture_output=True,
                )

            new_version = self.get_current_version()
            duration = time.time() - start_time

            logger.info(f"ETL rollback successful: {current_version} → {new_version}")

            return DeploymentResult(
                success=True,
                status=DeploymentStatus.ROLLED_BACK,
                old_version=current_version,
                new_version=new_version,
                duration_seconds=duration,
            )

        except Exception as e:
            duration = time.time() - start_time
            logger.exception(f"ETL rollback failed: {e}")

            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                old_version=current_version,
                duration_seconds=duration,
                error_message=f"Rollback failed: {e}",
            )
