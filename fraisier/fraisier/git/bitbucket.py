"""Bitbucket provider implementation."""

import hashlib
import hmac
from typing import Any

from .base import GitProvider, WebhookEvent


class BitbucketProvider(GitProvider):
    """Bitbucket Git provider.

    Supports:
    - bitbucket.org (Cloud)
    - Bitbucket Server/Data Center (self-hosted)
    """

    name = "bitbucket"

    def __init__(self, config: dict[str, Any]):
        super().__init__(config)
        self.webhook_secret = config.get("webhook_secret") or config.get("secret")
        self.is_server = config.get("server", False)  # Bitbucket Server vs Cloud

    def get_signature_header_name(self) -> str:
        if self.is_server:
            return "X-Hub-Signature"
        return "X-Hub-Signature"  # Cloud also uses this for IP allowlisting

    def get_event_header_name(self) -> str:
        return "X-Event-Key"

    def get_default_base_url(self) -> str:
        return "https://bitbucket.org"

    def verify_webhook_signature(self, payload: bytes, headers: dict[str, str]) -> bool:
        """Verify Bitbucket webhook signature.

        Note: Bitbucket Cloud primarily uses IP allowlisting.
        Bitbucket Server supports HMAC signatures.
        """
        if not self.webhook_secret:
            return True

        signature = headers.get(self.get_signature_header_name())
        if not signature:
            # Bitbucket Cloud may not send signature
            return not self.is_server

        # Remove prefix if present
        if signature.startswith("sha256="):
            signature = signature[7:]

        expected = hmac.new(
            self.webhook_secret.encode(),
            payload,
            hashlib.sha256,
        ).hexdigest()

        return hmac.compare_digest(expected, signature)

    def parse_webhook_event(self, headers: dict[str, str], payload: dict) -> WebhookEvent:
        """Parse Bitbucket webhook payload."""
        event_key = headers.get(self.get_event_header_name(), "unknown")

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

        # Extract sender (actor in Bitbucket terminology)
        actor = payload.get("actor", {})
        sender = actor.get("username") or actor.get("display_name")

        if event_key.startswith("repo:push"):
            is_push = True

            # Bitbucket sends changes array
            push_data = payload.get("push", {})
            changes = push_data.get("changes", [])

            if changes:
                change = changes[0]  # Take first change
                new = change.get("new", {})

                if new.get("type") == "branch":
                    branch = new.get("name")
                elif new.get("type") == "tag":
                    is_tag = True
                    branch = new.get("name")

                target = new.get("target", {})
                commit_sha = target.get("hash", "")[:12]

        elif event_key.startswith("pullrequest:"):
            is_merge_request = True
            pr_data = payload.get("pullrequest", {})
            source = pr_data.get("source", {})
            branch_data = source.get("branch", {})
            branch = branch_data.get("name")

            commit = source.get("commit", {})
            commit_sha = commit.get("hash", "")[:12]

        return WebhookEvent(
            provider=self.name,
            event_type=event_key,
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
