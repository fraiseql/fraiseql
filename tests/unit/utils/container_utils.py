"""Utilities for container-based testing."""

import shutil
import subprocess

import pytest


def check_docker_available() -> bool:
    """Check if Docker is available.

    Returns:
        True if Docker is available, False otherwise
    """
    if shutil.which("docker"):
        try:
            result = subprocess.run(
                ["docker", "info"], capture_output=True, text=True, check=False, timeout=5
            )
            if result.returncode == 0:
                return True
        except (subprocess.TimeoutExpired, OSError):
            pass
    return False


# Convenience decorator
requires_docker = pytest.mark.skipif(not check_docker_available(), reason="Docker not available")
