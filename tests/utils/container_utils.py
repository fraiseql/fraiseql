"""Utilities for container-based testing."""

import shutil
import subprocess
from typing import Literal

import pytest


def check_container_runtime() -> Literal["docker", "podman"] | None:
    """Check which container runtime is available.

    Returns:
        "docker" if Docker is available
        "podman" if Podman is available
        None if neither is available
    """
    # Check for Docker
    if shutil.which("docker"):
        try:
            result = subprocess.run(
                ["docker", "info"],
                capture_output=True,
                text=True,
                check=False,
                timeout=5,
            )
            if result.returncode == 0:
                return "docker"
        except (subprocess.TimeoutExpired, OSError):
            pass

    # Check for Podman
    if shutil.which("podman"):
        try:
            result = subprocess.run(
                ["podman", "info"],
                capture_output=True,
                text=True,
                check=False,
                timeout=5,
            )
            if result.returncode == 0:
                return "podman"
        except (subprocess.TimeoutExpired, OSError):
            pass

    return None


def requires_container_runtime(runtime: Literal["docker", "podman", "any"] = "any"):
    """Decorator to skip tests if container runtime is not available.

    Args:
        runtime: Required runtime - "docker", "podman", or "any"

    Example:
        @requires_container_runtime("docker")
        def test_docker_specific():
            ...

        @requires_container_runtime("any")
        def test_needs_containers():
            ...
    """
    available_runtime = check_container_runtime()

    if runtime == "any":
        skip_condition = available_runtime is None
        skip_reason = "No container runtime available (Docker or Podman)"
    elif runtime == "docker":
        skip_condition = available_runtime != "docker"
        skip_reason = "Docker not available"
    elif runtime == "podman":
        skip_condition = available_runtime != "podman"
        skip_reason = "Podman not available"
    else:
        raise ValueError(f"Invalid runtime: {runtime}")

    return pytest.mark.skipif(skip_condition, reason=skip_reason)


# Convenience markers
requires_docker = requires_container_runtime("docker")
requires_podman = requires_container_runtime("podman")
requires_any_container = requires_container_runtime("any")
