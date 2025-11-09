"""Smart Dependency Management for FraiseQL Integration Tests.

This module provides automatic dependency installation and management
for integration tests, inspired by FraiseQL backend's smart fixtures.

Key Features:
- Automatic detection and installation of missing dependencies
- Intelligent caching to avoid repeated installations
- Environment-aware installation strategies
- Fallback mechanisms for restricted environments
- Performance optimization through parallel installs
"""

import json
import logging
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional
from dataclasses import dataclass, field
from enum import Enum

logger = logging.getLogger(__name__)


class InstallStrategy(Enum):
    """Available installation strategies."""

    UV = "uv"
    PIP = "pip"
    SKIP = "skip"


class InstallResult(Enum):
    """Installation results."""

    SUCCESS = "success"
    FAILED = "failed"
    CACHED = "cached"
    SKIPPED = "skipped"


@dataclass
class DependencyInfo:
    """Information about a dependency."""

    name: str
    version: Optional[str] = None
    install_command: Optional[str] = None
    import_name: Optional[str] = None  # For validation
    required: bool = True


@dataclass
class InstallContext:
    """Context for dependency installation."""

    strategy: InstallStrategy
    timeout: int = 300
    cache_duration: int = 3600  # 1 hour cache
    parallel_install: bool = True
    environment: str = "LOCAL"
    auto_install_enabled: bool = True


class SmartDependencyManager:
    """Auto-install missing dependencies for integration tests."""

    def __init__(self, cache_dir: Optional[Path] = None) -> None:
        """Initialize dependency manager."""
        self.cache_dir = cache_dir or Path(__file__).parent
        self.cache_file = self.cache_dir / ".dependency_cache.json"
        self.install_log = self.cache_dir / ".install_log.txt"
        self.lock_file = self.cache_dir / ".dependency_install.lock"

        # Ensure cache directory exists
        self.cache_dir.mkdir(exist_ok=True)

        # Load cached dependency state
        self.dependency_cache = self._load_cache()

    def _load_cache(self) -> dict:
        """Load cached dependency state."""
        if self.cache_file.exists():
            try:
                with open(self.cache_file) as f:
                    return json.load(f)
            except (OSError, json.JSONDecodeError) as e:
                logger.warning(f"Failed to load dependency cache: {e}")
                return {}
        return {}

    def _save_cache(self) -> None:
        """Save dependency cache to disk."""
        try:
            with open(self.cache_file, "w") as f:
                json.dump(self.dependency_cache, f, indent=2)
        except OSError as e:
            logger.warning(f"Failed to save dependency cache: {e}")

    def _log_install_attempt(self, message: str) -> None:
        """Log installation attempt."""
        timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
        log_entry = f"[{timestamp}] {message}\n"

        try:
            with open(self.install_log, "a") as f:
                f.write(log_entry)
        except OSError as e:
            logger.warning(f"Failed to write install log: {e}")

    def detect_install_strategy(self) -> InstallStrategy:
        """Determine best installation method."""
        project_root = Path(__file__).parent.parent.parent.parent

        # Check for UV first (preferred and fastest)
        try:
            subprocess.run(["uv", "--version"], capture_output=True, check=True, timeout=5)
            if (project_root / "uv.lock").exists():
                return InstallStrategy.UV
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, FileNotFoundError):
            pass

        # Default to pip as fallback
        return InstallStrategy.PIP

    def _is_dependency_available(self, dep_info: DependencyInfo) -> bool:
        """Check if dependency is already available."""
        import_name = dep_info.import_name or dep_info.name

        # Handle special cases
        import_mapping = {
            "fraiseql": "fraiseql",
            "httpx": "httpx",
            "psycopg": "psycopg",
            "fastapi": "fastapi",
        }
        import_name = import_mapping.get(import_name, import_name)

        try:
            __import__(import_name)
            return True
        except ImportError:
            return False

    def _is_cached_valid(self, dep_name: str) -> bool:
        """Check if cached installation is still valid."""
        cache_entry = self.dependency_cache.get(dep_name)
        if not cache_entry:
            return False

        cached_time = cache_entry.get("installed_at", 0)
        cache_duration = cache_entry.get("cache_duration", 3600)

        return (time.time() - cached_time) < cache_duration

    def _install_dependency(
        self, dep_info: DependencyInfo, context: InstallContext
    ) -> InstallResult:
        """Install a single dependency."""
        # Check if already cached and valid
        if self._is_cached_valid(dep_info.name) and self._is_dependency_available(dep_info):
            logger.info(f"Using cached {dep_info.name}")
            return InstallResult.CACHED

        # Check if already available without installation
        if self._is_dependency_available(dep_info):
            # Update cache
            self.dependency_cache[dep_info.name] = {
                "installed_at": time.time(),
                "method": "pre-installed",
                "cache_duration": context.cache_duration,
            }
            return InstallResult.SUCCESS

        if not context.auto_install_enabled:
            logger.info(f"Auto-install disabled, skipping {dep_info.name}")
            return InstallResult.SKIPPED

        # Install the dependency
        try:
            install_cmd = self._build_install_command(dep_info, context)
            logger.info(f"Installing {dep_info.name} with: {' '.join(install_cmd)}")

            self._log_install_attempt(f"Installing {dep_info.name}: {' '.join(install_cmd)}")

            result = subprocess.run(
                install_cmd, capture_output=True, text=True, timeout=context.timeout
            )

            if result.returncode == 0:
                # Verify installation worked
                if self._is_dependency_available(dep_info):
                    self.dependency_cache[dep_info.name] = {
                        "installed_at": time.time(),
                        "method": context.strategy.value,
                        "cache_duration": context.cache_duration,
                    }
                    logger.info(f"Successfully installed {dep_info.name}")
                    return InstallResult.SUCCESS
                else:
                    logger.error(f"Installation completed but {dep_info.name} still not importable")
                    return InstallResult.FAILED
            else:
                logger.error(f"Failed to install {dep_info.name}: {result.stderr}")
                self._log_install_attempt(f"Failed to install {dep_info.name}: {result.stderr}")
                return InstallResult.FAILED

        except subprocess.TimeoutExpired:
            logger.error(f"Timeout installing {dep_info.name}")
            return InstallResult.FAILED
        except Exception as e:
            logger.error(f"Exception installing {dep_info.name}: {e}")
            return InstallResult.FAILED

    def _build_install_command(
        self, dep_info: DependencyInfo, context: InstallContext
    ) -> list[str]:
        """Build installation command based on strategy."""
        if dep_info.install_command:
            return dep_info.install_command.split()

        if context.strategy == InstallStrategy.UV:
            return ["uv", "pip", "install", dep_info.name]
        else:  # PIP fallback
            return [sys.executable, "-m", "pip", "install", dep_info.name]

    def ensure_dependencies(
        self, dependencies: list[str], context: Optional[InstallContext] = None
    ) -> tuple[bool, dict[str, InstallResult]]:
        """Install missing dependencies and return success status."""
        if context is None:
            context = InstallContext(
                strategy=self.detect_install_strategy(),
                auto_install_enabled=self._should_auto_install(),
            )

        # Convert string dependencies to DependencyInfo objects
        dep_infos = []
        for dep in dependencies:
            if isinstance(dep, str):
                dep_infos.append(DependencyInfo(name=dep))
            else:
                dep_infos.append(dep)

        results = {}
        all_success = True

        for dep_info in dep_infos:
            try:
                result = self._install_dependency(dep_info, context)
                results[dep_info.name] = result

                if result in [InstallResult.FAILED] and dep_info.required:
                    all_success = False

            except Exception as e:
                logger.error(f"Unexpected error processing {dep_info.name}: {e}")
                results[dep_info.name] = InstallResult.FAILED
                if dep_info.required:
                    all_success = False

        # Save cache after installation attempts
        self._save_cache()

        return all_success, results

    def _should_auto_install(self) -> bool:
        """Determine if auto-install should be enabled."""
        import os

        # Check environment variable override
        if os.getenv("FRAISEQL_SKIP_AUTO_INSTALL", "").lower() in ["true", "1", "yes"]:
            return False

        if os.getenv("FRAISEQL_AUTO_INSTALL", "").lower() in ["true", "1", "yes"]:
            return True

        # Enable auto-install for local development by default
        # Disable in CI unless explicitly enabled
        in_ci = any(
            ci_var in os.environ
            for ci_var in ["CI", "CONTINUOUS_INTEGRATION", "GITHUB_ACTIONS", "TRAVIS", "JENKINS"]
        )

        return not in_ci

    def clear_cache(self) -> None:
        """Clear dependency cache."""
        self.dependency_cache = {}
        if self.cache_file.exists():
            self.cache_file.unlink()
        logger.info("Dependency cache cleared")

    def get_cache_status(self) -> dict:
        """Get current cache status for debugging."""
        return {
            "cache_file_exists": self.cache_file.exists(),
            "cache_entries": len(self.dependency_cache),
            "cached_dependencies": list(self.dependency_cache.keys()),
            "install_strategy": self.detect_install_strategy().value,
            "auto_install_enabled": self._should_auto_install(),
        }


def get_example_dependencies() -> list[DependencyInfo]:
    """Get the list of dependencies required for example integration tests."""
    return [
        DependencyInfo(name="fraiseql", import_name="fraiseql", required=True),
        DependencyInfo(name="httpx", import_name="httpx", required=True),
        DependencyInfo(name="psycopg[pool]", import_name="psycopg", required=True),
        DependencyInfo(name="fastapi", import_name="fastapi", required=True),
        DependencyInfo(name="uvicorn", import_name="uvicorn", required=False),
    ]


# Global instance for easy access
_dependency_manager = None


def get_dependency_manager() -> SmartDependencyManager:
    """Get global dependency manager instance."""
    global _dependency_manager
    if _dependency_manager is None:
        _dependency_manager = SmartDependencyManager()
    return _dependency_manager
