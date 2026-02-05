"""Git provider abstraction for Fraisier.

Supports any Git platform: GitHub, GitLab, Gitea, Bitbucket, or self-hosted.
"""

from .base import GitProvider, WebhookEvent
from .registry import get_provider, list_providers, register_provider

__all__ = [
    "GitProvider",
    "WebhookEvent",
    "get_provider",
    "list_providers",
    "register_provider",
]
