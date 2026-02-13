"""Configuration loader for Fraisier deployment system.

Loads fraise definitions from fraises.yaml.
Supports hierarchical fraise -> environment structure.
"""

from pathlib import Path
from typing import Any

import yaml


class FraisierConfig:
    """Load and manage deployment configuration from fraises.yaml.

    Supports hierarchical structure:
        fraises:
          <fraise_name>:
            type: api|etl|scheduled|backup
            environments:
              <env_name>:
                <config>
    """

    def __init__(self, config_path: Path | str | None = None):
        """Initialize configuration.

        Args:
            config_path: Path to fraises.yaml. If None, uses default locations.
        """
        self.config_path = self._resolve_config_path(config_path)
        self._config: dict[str, Any] = {}
        self._load()

    def _resolve_config_path(self, config_path: Path | str | None) -> Path:
        """Resolve configuration file path."""
        if config_path:
            return Path(config_path)

        # Check standard locations
        locations = [
            Path("/opt/fraisier/fraises.yaml"),
            Path.cwd() / "fraises.yaml",
            Path.cwd() / "config" / "fraises.yaml",
            Path(__file__).parent.parent / "fraises.yaml",
        ]

        for loc in locations:
            if loc.exists():
                return loc

        raise FileNotFoundError(
            f"fraises.yaml not found in any of: {[str(p) for p in locations]}"
        )

    def _load(self) -> None:
        """Load configuration from YAML file."""
        with open(self.config_path) as f:
            self._config = yaml.safe_load(f)

    def reload(self) -> None:
        """Reload configuration from file."""
        self._load()

    @property
    def fraises(self) -> dict[str, dict[str, Any]]:
        """Get all fraise configurations."""
        return self._config.get("fraises", {})

    @property
    def environments(self) -> dict[str, dict[str, Any]]:
        """Get global environment configurations."""
        return self._config.get("environments", {})

    @property
    def branch_mapping(self) -> dict[str, dict[str, str]]:
        """Get branch to fraise/environment mapping."""
        return self._config.get("branch_mapping", {})

    def get_fraise(self, fraise_name: str) -> dict[str, Any] | None:
        """Get configuration for a fraise (all environments)."""
        return self.fraises.get(fraise_name)

    def get_fraise_environment(
        self, fraise_name: str, environment: str
    ) -> dict[str, Any] | None:
        """Get configuration for a specific fraise + environment.

        Args:
            fraise_name: e.g., "my_api", "etl", "backup"
            environment: e.g., "development", "staging", "production"

        Returns:
            Merged config with fraise-level and environment-level settings
        """
        fraise = self.fraises.get(fraise_name)
        if not fraise:
            return None

        env_config = fraise.get("environments", {}).get(environment)
        if not env_config:
            return None

        # Merge fraise-level config with environment-specific config
        return {
            "fraise_name": fraise_name,
            "environment": environment,
            "type": fraise.get("type"),
            "description": fraise.get("description"),
            **env_config,
        }

    def get_fraise_for_branch(self, branch: str) -> dict[str, Any] | None:
        """Get fraise configuration for a git branch (webhook routing).

        Args:
            branch: Git branch name (e.g., "dev", "main")

        Returns:
            Full fraise+environment config for the branch
        """
        mapping = self.branch_mapping.get(branch)
        if not mapping:
            return None

        fraise_name = mapping.get("fraise")
        environment = mapping.get("environment")

        if not fraise_name or not environment:
            return None

        return self.get_fraise_environment(fraise_name, environment)

    def list_fraises(self) -> list[dict[str, Any]]:
        """List all fraises with their environments.

        Returns:
            List of fraise summaries with environment info
        """
        result = []
        for fraise_name, fraise in self.fraises.items():
            environments = list(fraise.get("environments", {}).keys())
            result.append({
                "name": fraise_name,
                "type": fraise.get("type", "unknown"),
                "description": fraise.get("description", ""),
                "environments": environments,
            })
        return result

    def list_all_deployments(self) -> list[dict[str, Any]]:
        """List all fraise+environment combinations (deployable targets).

        Returns:
            List of all deployable targets
        """
        result = []
        for fraise_name, fraise in self.fraises.items():
            fraise_type = fraise.get("type", "unknown")
            description = fraise.get("description", "")

            for env_name, env_config in fraise.get("environments", {}).items():
                # Handle fraises with nested jobs (backup, statistics)
                if "jobs" in env_config:
                    for job_name, job_config in env_config["jobs"].items():
                        result.append({
                            "fraise": fraise_name,
                            "environment": env_name,
                            "job": job_name,
                            "type": fraise_type,
                            "name": job_config.get("name", job_name),
                            "description": job_config.get("description", description),
                        })
                else:
                    result.append({
                        "fraise": fraise_name,
                        "environment": env_name,
                        "job": None,
                        "type": fraise_type,
                        "name": env_config.get("name", fraise_name),
                        "description": description,
                    })
        return result

    def get_deployments_by_type(self, fraise_type: str) -> list[dict[str, Any]]:
        """Get all deployments of a specific type."""
        return [d for d in self.list_all_deployments() if d["type"] == fraise_type]

    def get_deployments_by_environment(self, environment: str) -> list[dict[str, Any]]:
        """Get all deployments for a specific environment."""
        return [d for d in self.list_all_deployments() if d["environment"] == environment]


# Global config instance (lazy loaded)
_config: FraisierConfig | None = None


def get_config(config_path: Path | str | None = None) -> FraisierConfig:
    """Get or create global configuration instance."""
    global _config
    if _config is None or config_path:
        _config = FraisierConfig(config_path)
    return _config
