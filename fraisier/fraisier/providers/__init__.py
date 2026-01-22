"""Deployment providers for multiple infrastructure types.

Supports:
- Bare Metal (SSH + systemd)
- Docker Compose
- Coolify
"""

from .bare_metal import BareMetalProvider
from .base import (
    DeploymentProvider,
    HealthCheck,
    HealthCheckType,
    ProviderStatus,
    ProviderType,
)
from .coolify import CoolifyProvider
from .docker_compose import DockerComposeProvider

__all__ = [
    "DeploymentProvider",
    "BareMetalProvider",
    "DockerComposeProvider",
    "CoolifyProvider",
    "ProviderType",
    "ProviderStatus",
    "HealthCheck",
    "HealthCheckType",
]
