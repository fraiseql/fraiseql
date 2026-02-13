"""GitLab provider implementation."""

import hmac
from typing import Any

from .base import GitProvider, WebhookEvent


class GitLabProvider(GitProvider):
    """GitLab Git provider.

    Supports:
    - gitlab.com (default)
    - Self-hosted GitLab instances
    """

    name = "gitlab"

    def __init__(self, config: dict[str, Any]):
        super().__init__(config)
        self.webhook_secret = config.get("webhook_secret") or config.get("secret_token")

    def get_signature_header_name(self) -> str:
        return "X-Gitlab-Token"

    def get_event_header_name(self) -> str:
        return "X-Gitlab-Event"

    def get_default_base_url(self) -> str:
        return "https://gitlab.com"

    def verify_webhook_signature(self, payload: bytes, headers: dict[str, str]) -> bool:
        """Verify GitLab webhook token.

        GitLab uses a simple token comparison, not HMAC.
        """
        if not self.webhook_secret:
            return True

        token = headers.get(self.get_signature_header_name())
        if not token:
            return False

        return hmac.compare_digest(self.webhook_secret, token)

    def parse_webhook_event(self, headers: dict[str, str], payload: dict) -> WebhookEvent:
        """Parse GitLab webhook payload."""
        event_type = headers.get(self.get_event_header_name(), "unknown")

        # GitLab also sends object_kind in payload
        object_kind = payload.get("object_kind", "")

        branch = None
        commit_sha = None
        sender = None
        repository = None
        is_push = False
        is_tag = False
        is_merge_request = False
        is_ping = False

        # Extract repository
        project = payload.get("project", {})
        repository = project.get("path_with_namespace")

        # Extract sender
        user = payload.get("user", {}) or payload.get("user_username")
        if isinstance(user, dict):
            sender = user.get("username")
        else:
            sender = user

        if object_kind == "push" or event_type == "Push Hook":
            is_push = True
            ref = payload.get("ref", "")

            if ref.startswith("refs/heads/"):
                branch = ref.replace("refs/heads/", "")
            elif ref.startswith("refs/tags/"):
                is_tag = True
                branch = ref.replace("refs/tags/", "")

            commit_sha = payload.get("after") or payload.get("checkout_sha")

        elif object_kind == "tag_push" or event_type == "Tag Push Hook":
            is_push = True
            is_tag = True
            ref = payload.get("ref", "")
            branch = ref.replace("refs/tags/", "")
            commit_sha = payload.get("checkout_sha")

        elif object_kind == "merge_request" or event_type == "Merge Request Hook":
            is_merge_request = True
            mr = payload.get("object_attributes", {})
            branch = mr.get("source_branch")
            commit_sha = mr.get("last_commit", {}).get("id")

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
