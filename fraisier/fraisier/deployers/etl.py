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
