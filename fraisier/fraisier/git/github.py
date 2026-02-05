"""GitHub provider implementation."""

import hashlib
import hmac
from typing import Any

from .base import GitProvider, WebhookEvent


class GitHubProvider(GitProvider):
    """GitHub Git provider.

    Supports:
    - github.com (default)
    - GitHub Enterprise (self-hosted)
    """

    name = "github"

    def __init__(self, config: dict[str, Any]):
        super().__init__(config)
        # Support both naming conventions
        self.webhook_secret = config.get("webhook_secret") or config.get("secret")

    def get_signature_header_name(self) -> str:
        return "X-Hub-Signature-256"

    def get_event_header_name(self) -> str:
        return "X-GitHub-Event"

    def get_default_base_url(self) -> str:
        return "https://github.com"

    def verify_webhook_signature(self, payload: bytes, headers: dict[str, str]) -> bool:
        """Verify GitHub webhook signature using HMAC-SHA256."""
        if not self.webhook_secret:
            return True  # No secret configured, skip verification

        signature = headers.get(self.get_signature_header_name())
        if not signature:
            return False

        expected = "sha256=" + hmac.new(
            self.webhook_secret.encode(),
            payload,
            hashlib.sha256,
        ).hexdigest()

        return hmac.compare_digest(expected, signature)

    def parse_webhook_event(self, headers: dict[str, str], payload: dict) -> WebhookEvent:
        """Parse GitHub webhook payload."""
        event_type = headers.get(self.get_event_header_name(), "unknown")

        branch = None
        commit_sha = None
        sender = None
        repository = None
        is_push = False
        is_tag = False
        is_merge_request = False
        is_ping = False

        # Extract repository
        repo_data = payload.get("repository", {})
        repository = repo_data.get("full_name")

        # Extract sender
        sender_data = payload.get("sender", {})
        sender = sender_data.get("login")

        if event_type == "push":
            is_push = True
            ref = payload.get("ref", "")

            if ref.startswith("refs/heads/"):
                branch = ref.replace("refs/heads/", "")
            elif ref.startswith("refs/tags/"):
                is_tag = True
                branch = ref.replace("refs/tags/", "")

            commit_sha = payload.get("after")

        elif event_type == "pull_request":
            is_merge_request = True
            pr_data = payload.get("pull_request", {})
            head = pr_data.get("head", {})
            branch = head.get("ref")
            commit_sha = head.get("sha")

        elif event_type == "ping":
            is_ping = True

        return WebhookEvent(
            provider=self.name,
            event_type=event_type,
            branch=branch,
            commit_sha=commit_sha,
            sender=sender,
            repository=repository,
            raw_payload=payload,
            is_push=is_push,
            is_tag=is_tag,
            is_merge_request=is_merge_request,
            is_ping=is_ping,
        )
