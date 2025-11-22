"""Environment Detection for FraiseQL Integration Tests.

This module provides intelligent environment detection and strategy selection
for integration tests, helping determine the optimal testing approach based
on the current execution context.

Key Features:
- Automatic environment detection (LOCAL, CI, DEV, STAGING, PROD)
- Strategy selection for dependency management and database setup
- Performance optimization based on environment capabilities
- Configuration management for different environments
"""

import os
import platform
import socket
import subprocess
import sys
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Optional, Any


class Environment(Enum):
    """Detected environments."""

    LOCAL = "local"  # Developer machine
    CI = "ci"  # Continuous Integration (GitHub Actions, etc.)
    DEV = "dev"  # Development server
    STAGING = "staging"  # Staging environment
    PRODUCTION = "production"  # Production environment
    UNKNOWN = "unknown"  # Could not determine


class PerformanceProfile(Enum):
    """Performance profiles for different environments."""

    HIGH = "high"  # Fast machines, parallel processing
    MEDIUM = "medium"  # Standard capabilities
    LOW = "low"  # Constrained resources
    CONSTRAINED = "constrained"  # Very limited resources


@dataclass
class EnvironmentConfig:
    """Configuration for a specific environment."""

    environment: Environment
    performance_profile: PerformanceProfile
    auto_install_dependencies: bool
    use_database_templates: bool
    parallel_execution: bool
    timeout_multiplier: float
    cache_duration: int  # seconds
    max_parallel_installs: int
    database_strategy: str
    logging_level: str

    # Timeout configurations
    dependency_install_timeout: int = 300
    database_setup_timeout: int = 120
    test_execution_timeout: int = 60


class EnvironmentDetector:
    """Detect environment and select appropriate testing strategy."""

    def __init__(self) -> None:
        """Initialize environment detector."""
        self._cached_environment = None
        self._cached_config = None

    def detect_environment(self) -> Environment:
        """Detect the current environment."""
        if self._cached_environment is not None:
            return self._cached_environment

        # Check for explicit environment variable
        env_var = os.getenv("FRAISEQL_ENVIRONMENT", "").lower()
        if env_var:
            try:
                self._cached_environment = Environment(env_var)
                return self._cached_environment
            except ValueError:
                pass

        # Detect CI environments
        if self._is_ci_environment():
            self._cached_environment = Environment.CI
            return self._cached_environment

        # Detect development/staging/production servers
        server_env = self._detect_server_environment()
        if server_env != Environment.UNKNOWN:
            self._cached_environment = server_env
            return self._cached_environment

        # Default to local development
        self._cached_environment = Environment.LOCAL
        return self._cached_environment

    def _is_ci_environment(self) -> bool:
        """Check if running in CI environment."""
        ci_indicators = [
            "CI",
            "CONTINUOUS_INTEGRATION",
            "GITHUB_ACTIONS",
            "TRAVIS",
            "CIRCLECI",
            "JENKINS_URL",
            "BUILDKITE",
            "GITLAB_CI",
            "AZURE_HTTP_USER_AGENT",
            "TF_BUILD",  # Azure DevOps
            "APPVEYOR",
            "DRONE",
        ]

        return any(indicator in os.environ for indicator in ci_indicators)

    def _detect_server_environment(self) -> Environment:
        """Detect server environment based on various indicators."""
        # Check hostname patterns
        hostname = socket.gethostname().lower()

        if any(pattern in hostname for pattern in ["dev", "development"]):
            return Environment.DEV
        elif any(pattern in hostname for pattern in ["staging", "stage"]):
            return Environment.STAGING
        elif any(pattern in hostname for pattern in ["prod", "production"]):
            return Environment.PRODUCTION

        # Check for environment-specific files or paths
        if Path("/app").exists() and Path("/app/docker-compose.yml").exists():
            # Likely containerized environment
            compose_content = ""
            try:
                with open("/app/docker-compose.yml") as f:
                    compose_content = f.read().lower()
                if "staging" in compose_content:
                    return Environment.STAGING
                elif "production" in compose_content:
                    return Environment.PRODUCTION
                else:
                    return Environment.DEV
            except:
                pass

        # Check environment variables for deployment indicators
        if os.getenv("DEPLOYMENT_ENVIRONMENT"):
            env_name = os.getenv("DEPLOYMENT_ENVIRONMENT").lower()
            if "staging" in env_name:
                return Environment.STAGING
            elif "production" in env_name:
                return Environment.PRODUCTION
            elif "dev" in env_name:
                return Environment.DEV

        return Environment.UNKNOWN

    def detect_performance_profile(self) -> PerformanceProfile:
        """Detect performance capabilities of current environment."""
        # Check CPU count
        cpu_count = os.cpu_count() or 1

        # Check available memory (approximate)
        available_memory = self._get_available_memory_gb()

        # CI environments are typically well-resourced
        if self.detect_environment() == Environment.CI:
            return PerformanceProfile.HIGH

        # Performance classification based on resources
        if cpu_count >= 8 and available_memory >= 16:
            return PerformanceProfile.HIGH
        elif cpu_count >= 4 and available_memory >= 8:
            return PerformanceProfile.MEDIUM
        elif cpu_count >= 2 and available_memory >= 4:
            return PerformanceProfile.LOW
        else:
            return PerformanceProfile.CONSTRAINED

    def _get_available_memory_gb(self) -> float:
        """Get available memory in GB (approximate)."""
        try:
            if platform.system() == "Linux":
                with open("/proc/meminfo") as f:
                    for line in f:
                        if line.startswith("MemAvailable:"):
                            kb = int(line.split()[1])
                            return kb / 1024 / 1024  # Convert to GB
                        elif line.startswith("MemTotal:"):
                            kb = int(line.split()[1])
                            return kb / 1024 / 1024 * 0.8  # Assume 80% available
            elif platform.system() == "Darwin":  # macOS
                result = subprocess.run(["sysctl", "hw.memsize"], capture_output=True, text=True)
                if result.returncode == 0:
                    bytes_mem = int(result.stdout.split()[1])
                    return bytes_mem / 1024 / 1024 / 1024
        except:
            pass

        # Default assumption
        return 8.0

    def get_environment_config(self) -> EnvironmentConfig:
        """Get configuration for current environment."""
        if self._cached_config is not None:
            return self._cached_config

        environment = self.detect_environment()
        performance = self.detect_performance_profile()

        # Base configurations per environment
        configs = {
            Environment.LOCAL: EnvironmentConfig(
                environment=Environment.LOCAL,
                performance_profile=performance,
                auto_install_dependencies=True,
                use_database_templates=True,
                parallel_execution=performance
                in [PerformanceProfile.HIGH, PerformanceProfile.MEDIUM],
                timeout_multiplier=2.0,  # More lenient for first-time setups
                cache_duration=3600,  # 1 hour
                max_parallel_installs=min(4, os.cpu_count() or 2),
                database_strategy="template_clone",
                logging_level="INFO",
            ),
            Environment.CI: EnvironmentConfig(
                environment=Environment.CI,
                performance_profile=PerformanceProfile.HIGH,
                auto_install_dependencies=False,  # Dependencies should be pre-installed
                use_database_templates=True,
                parallel_execution=True,
                timeout_multiplier=1.5,
                cache_duration=1800,  # 30 minutes
                max_parallel_installs=6,
                database_strategy="template_clone",
                logging_level="INFO",
            ),
            Environment.DEV: EnvironmentConfig(
                environment=Environment.DEV,
                performance_profile=performance,
                auto_install_dependencies=False,
                use_database_templates=True,
                parallel_execution=performance == PerformanceProfile.HIGH,
                timeout_multiplier=1.5,
                cache_duration=1800,
                max_parallel_installs=3,
                database_strategy="reuse_existing",
                logging_level="DEBUG",
            ),
            Environment.STAGING: EnvironmentConfig(
                environment=Environment.STAGING,
                performance_profile=performance,
                auto_install_dependencies=False,
                use_database_templates=False,
                parallel_execution=False,
                timeout_multiplier=1.0,
                cache_duration=900,  # 15 minutes
                max_parallel_installs=2,
                database_strategy="direct_setup",
                logging_level="WARNING",
            ),
            Environment.PRODUCTION: EnvironmentConfig(
                environment=Environment.PRODUCTION,
                performance_profile=performance,
                auto_install_dependencies=False,
                use_database_templates=False,
                parallel_execution=False,
                timeout_multiplier=1.0,
                cache_duration=300,  # 5 minutes
                max_parallel_installs=1,
                database_strategy="skip",
                logging_level="ERROR",
            ),
        }

        base_config = configs.get(environment, configs[Environment.LOCAL])

        # Apply performance profile adjustments
        if performance == PerformanceProfile.CONSTRAINED:
            base_config.parallel_execution = False
            base_config.max_parallel_installs = 1
            base_config.timeout_multiplier *= 2
        elif performance == PerformanceProfile.HIGH:
            base_config.max_parallel_installs = min(8, base_config.max_parallel_installs * 2)

        # Apply environment variable overrides
        base_config = self._apply_env_overrides(base_config)

        self._cached_config = base_config
        return base_config

    def _apply_env_overrides(self, config: EnvironmentConfig) -> EnvironmentConfig:
        """Apply environment variable overrides to configuration."""
        # Auto-install override
        if os.getenv("FRAISEQL_AUTO_INSTALL"):
            config.auto_install_dependencies = os.getenv("FRAISEQL_AUTO_INSTALL").lower() in [
                "true",
                "1",
                "yes",
            ]

        # Database strategy override
        if os.getenv("FRAISEQL_DATABASE_STRATEGY"):
            config.database_strategy = os.getenv("FRAISEQL_DATABASE_STRATEGY")

        # Timeout multiplier override
        if os.getenv("FRAISEQL_TIMEOUT_MULTIPLIER"):
            try:
                config.timeout_multiplier = float(os.getenv("FRAISEQL_TIMEOUT_MULTIPLIER"))
            except ValueError:
                pass

        # Parallel execution override
        if os.getenv("FRAISEQL_PARALLEL_EXECUTION"):
            config.parallel_execution = os.getenv("FRAISEQL_PARALLEL_EXECUTION").lower() in [
                "true",
                "1",
                "yes",
            ]

        # Logging level override
        if os.getenv("FRAISEQL_LOG_LEVEL"):
            config.logging_level = os.getenv("FRAISEQL_LOG_LEVEL").upper()

        return config

    def should_auto_install(self) -> bool:
        """Determine if auto-install should be enabled."""
        return self.get_environment_config().auto_install_dependencies

    def should_use_database_templates(self) -> bool:
        """Determine if database templates should be used."""
        return self.get_environment_config().use_database_templates

    def get_timeout_config(self) -> dict[str, int]:
        """Get timeout configuration for current environment."""
        config = self.get_environment_config()
        multiplier = config.timeout_multiplier

        return {
            "dependency_install": int(config.dependency_install_timeout * multiplier),
            "database_setup": int(config.database_setup_timeout * multiplier),
            "test_execution": int(config.test_execution_timeout * multiplier),
        }

    def get_resource_limits(self) -> dict[str, int]:
        """Get resource limits for current environment."""
        config = self.get_environment_config()

        return {
            "max_parallel_installs": config.max_parallel_installs,
            "max_parallel_tests": config.max_parallel_installs,  # Same as installs for now
            "cache_duration": config.cache_duration,
        }

    def is_development_environment(self) -> bool:
        """Check if this is a development environment (LOCAL or DEV)."""
        env = self.detect_environment()
        return env in [Environment.LOCAL, Environment.DEV]

    def is_production_like_environment(self) -> bool:
        """Check if this is a production-like environment."""
        env = self.detect_environment()
        return env in [Environment.STAGING, Environment.PRODUCTION]

    def get_debug_info(self) -> dict[str, Any]:
        """Get debugging information about environment detection."""
        config = self.get_environment_config()

        return {
            "detected_environment": self.detect_environment().value,
            "performance_profile": self.detect_performance_profile().value,
            "hostname": socket.gethostname(),
            "platform": platform.system(),
            "python_version": sys.version,
            "cpu_count": os.cpu_count(),
            "available_memory_gb": self._get_available_memory_gb(),
            "is_ci": self._is_ci_environment(),
            "config": {
                "auto_install_dependencies": config.auto_install_dependencies,
                "use_database_templates": config.use_database_templates,
                "parallel_execution": config.parallel_execution,
                "timeout_multiplier": config.timeout_multiplier,
                "max_parallel_installs": config.max_parallel_installs,
                "database_strategy": config.database_strategy,
                "logging_level": config.logging_level,
            },
        }


# Global instance for easy access
_environment_detector = None


def get_environment_detector() -> EnvironmentDetector:
    """Get global environment detector instance."""
    global _environment_detector
    if _environment_detector is None:
        _environment_detector = EnvironmentDetector()
    return _environment_detector


def get_current_environment() -> Environment:
    """Get current environment (convenience function)."""
    return get_environment_detector().detect_environment()


def get_environment_config() -> EnvironmentConfig:
    """Get current environment configuration (convenience function)."""
    return get_environment_detector().get_environment_config()
