"""Abstract Git provider interface.

Any Git platform (GitHub, GitLab, Gitea, Bitbucket, self-hosted) can be
supported by implementing this interface.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any


@dataclass
class WebhookEvent:
    """Normalized webhook event from any Git provider."""

    provider: str           # "github", "gitlab", "gitea", "bitbucket", etc.
    event_type: str         # "push", "merge_request", "pull_request", "tag", etc.
    branch: str | None      # Branch name (for push events)
    commit_sha: str | None  # Commit SHA
    sender: str | None      # Username who triggered the event
    repository: str | None  # Repository name (owner/repo)
    raw_payload: dict       # Original payload for provider-specific data

    # Normalized event types
    is_push: bool = False
    is_tag: bool = False
    is_merge_request: bool = False  # PR/MR
    is_ping: bool = False


class GitProvider(ABC):
    """Abstract base class for Git providers.

    Implement this interface to add support for any Git hosting platform.
    """

    name: str  # Provider identifier (e.g., "github", "gitlab")

    def __init__(self, config: dict[str, Any]):
        """Initialize provider with configuration.

        Args:
            config: Provider-specific configuration from fraises.yaml
        """
        self.config = config
        self.webhook_secret = config.get("webhook_secret")

    @abstractmethod
    def verify_webhook_signature(self, payload: bytes, headers: dict[str, str]) -> bool:
        """Verify webhook signature.

        Args:
            payload: Raw request body
            headers: Request headers

        Returns:
            True if signature is valid
        """
        pass

    @abstractmethod
    def parse_webhook_event(self, headers: dict[str, str], payload: dict) -> WebhookEvent:
        """Parse webhook payload into normalized event.

        Args:
            headers: Request headers
            payload: Parsed JSON payload

        Returns:
            Normalized WebhookEvent
        """
        pass

    @abstractmethod
    def get_signature_header_name(self) -> str:
        """Get the header name containing the webhook signature.

        Returns:
            Header name (e.g., "X-Hub-Signature-256" for GitHub)
        """
        pass

    @abstractmethod
    def get_event_header_name(self) -> str:
        """Get the header name containing the event type.

        Returns:
            Header name (e.g., "X-GitHub-Event" for GitHub)
        """
        pass

    def get_clone_url(self, repository: str) -> str:
        """Get clone URL for a repository.

        Args:
            repository: Repository identifier (e.g., "owner/repo")

        Returns:
            Git clone URL
        """
        base_url = self.config.get("base_url", self.get_default_base_url())
        return f"{base_url}/{repository}.git"

    @abstractmethod
    def get_default_base_url(self) -> str:
        """Get default base URL for this provider.

        Returns:
            Base URL (e.g., "https://github.com")
        """
        pass
