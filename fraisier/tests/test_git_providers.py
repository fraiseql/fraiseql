"""Tests for git provider implementations."""

import hashlib
import hmac
import json
from unittest.mock import MagicMock, patch

import pytest

from fraisier.git.github import GitHub
from fraisier.git.gitlab import GitLab
from fraisier.git.gitea import Gitea
from fraisier.git.bitbucket import Bitbucket


class TestGitHubProvider:
    """Tests for GitHub provider."""

    def test_init(self):
        """Test GitHub provider initialization."""
        config = {"webhook_secret": "test-secret"}
        provider = GitHub(config)

        assert provider.name == "github"
        assert provider.webhook_secret == "test-secret"

    def test_verify_webhook_signature_valid(self):
        """Test webhook signature verification with valid signature."""
        secret = b"test-secret"
        payload = b'{"test": "data"}'

        # Create correct signature
        signature = "sha256=" + hmac.new(
            secret, payload, hashlib.sha256
        ).hexdigest()

        provider = GitHub({"webhook_secret": secret.decode()})

        # Verify signature
        result = provider.verify_webhook_signature(payload, {
            "X-Hub-Signature-256": signature
        })

        assert result is True

    def test_verify_webhook_signature_invalid(self):
        """Test webhook signature verification with invalid signature."""
        provider = GitHub({"webhook_secret": "correct-secret"})

        result = provider.verify_webhook_signature(
            b'{"test": "data"}',
            {"X-Hub-Signature-256": "sha256=invalidsignature"}
        )

        assert result is False

    def test_verify_webhook_signature_missing_header(self):
        """Test webhook signature verification with missing signature header."""
        provider = GitHub({"webhook_secret": "secret"})

        result = provider.verify_webhook_signature(b'{"test": "data"}', {})

        assert result is False

    def test_parse_webhook_push_event(self):
        """Test parsing GitHub push event."""
        provider = GitHub({"webhook_secret": "secret"})

        payload = {
            "ref": "refs/heads/main",
            "repository": {"full_name": "user/repo"},
            "pusher": {"name": "user"},
            "head_commit": {"id": "abc123def456"},
        }
        headers = {"X-GitHub-Event": "push"}

        event = provider.parse_webhook_event(headers, payload)

        assert event.branch == "main"
        assert event.commit_sha == "abc123def456"
        assert event.sender == "user"
        assert event.is_push is True

    def test_parse_webhook_ping_event(self):
        """Test parsing GitHub ping event."""
        provider = GitHub({"webhook_secret": "secret"})

        payload = {"zen": "Design for failure"}
        headers = {"X-GitHub-Event": "ping"}

        event = provider.parse_webhook_event(headers, payload)

        assert event.event_type == "ping"

    def test_parse_webhook_pull_request_event(self):
        """Test parsing GitHub pull request event."""
        provider = GitHub({"webhook_secret": "secret"})

        payload = {
            "action": "opened",
            "pull_request": {
                "head": {
                    "ref": "feature-branch",
                    "sha": "xyz789",
                },
                "user": {"login": "developer"},
            },
        }
        headers = {"X-GitHub-Event": "pull_request"}

        event = provider.parse_webhook_event(headers, payload)

        assert event.branch == "feature-branch"
        assert event.event_type == "pull_request"


class TestGitLabProvider:
    """Tests for GitLab provider."""

    def test_init(self):
        """Test GitLab provider initialization."""
        config = {"webhook_secret": "test-secret"}
        provider = GitLab(config)

        assert provider.name == "gitlab"

    def test_verify_webhook_signature_valid(self):
        """Test GitLab webhook signature verification."""
        secret = "test-secret"
        payload = b'{"test": "data"}'

        # GitLab uses X-Gitlab-Token header
        provider = GitLab({"webhook_secret": secret})

        result = provider.verify_webhook_signature(payload, {
            "X-Gitlab-Token": secret
        })

        assert result is True

    def test_verify_webhook_signature_invalid(self):
        """Test GitLab webhook verification with invalid token."""
        provider = GitLab({"webhook_secret": "correct-secret"})

        result = provider.verify_webhook_signature(
            b'{"test": "data"}',
            {"X-Gitlab-Token": "wrong-token"}
        )

        assert result is False

    def test_parse_webhook_push_event(self):
        """Test parsing GitLab push event."""
        provider = GitLab({"webhook_secret": "secret"})

        payload = {
            "ref": "refs/heads/main",
            "user_username": "developer",
            "checkout_sha": "abc123",
            "project": {"name": "my-project"},
        }
        headers = {"X-Gitlab-Event": "Push Hook"}

        event = provider.parse_webhook_event(headers, payload)

        assert event.branch == "main"
        assert event.commit_sha == "abc123"
        assert event.sender == "developer"


class TestGiteaProvider:
    """Tests for Gitea provider."""

    def test_init(self):
        """Test Gitea provider initialization."""
        config = {"webhook_secret": "test-secret"}
        provider = Gitea(config)

        assert provider.name == "gitea"

    def test_verify_webhook_signature_valid(self):
        """Test Gitea webhook signature verification."""
        secret = b"test-secret"
        payload = b'{"test": "data"}'

        # Gitea uses sha256 HMAC like GitHub
        signature = "sha256=" + hmac.new(
            secret, payload, hashlib.sha256
        ).hexdigest()

        provider = Gitea({"webhook_secret": secret.decode()})

        result = provider.verify_webhook_signature(payload, {
            "X-Gitea-Signature": signature
        })

        assert result is True

    def test_parse_webhook_push_event(self):
        """Test parsing Gitea push event."""
        provider = Gitea({"webhook_secret": "secret"})

        payload = {
            "ref": "refs/heads/main",
            "pusher": {"username": "developer"},
            "after": "abc123def456",
        }
        headers = {"X-Gitea-Event": "push"}

        event = provider.parse_webhook_event(headers, payload)

        assert event.branch == "main"
        assert event.commit_sha == "abc123de"
        assert event.sender == "developer"


class TestBitbucketProvider:
    """Tests for Bitbucket provider."""

    def test_init(self):
        """Test Bitbucket provider initialization."""
        config = {"webhook_secret": "test-secret"}
        provider = Bitbucket(config)

        assert provider.name == "bitbucket"

    def test_verify_webhook_signature_valid(self):
        """Test Bitbucket webhook signature verification."""
        secret = b"test-secret"
        payload = b'{"test": "data"}'

        # Bitbucket uses sha256 HMAC
        signature = "sha256=" + hmac.new(
            secret, payload, hashlib.sha256
        ).hexdigest()

        provider = Bitbucket({"webhook_secret": secret.decode()})

        result = provider.verify_webhook_signature(payload, {
            "X-Hub-Signature": signature
        })

        assert result is True

    def test_parse_webhook_push_event(self):
        """Test parsing Bitbucket push event."""
        provider = Bitbucket({"webhook_secret": "secret"})

        payload = {
            "push": {
                "changes": [
                    {
                        "new": {
                            "name": "main",
                            "target": {"hash": "abc123def456"},
                        },
                    }
                ]
            },
            "actor": {"username": "developer"},
        }
        headers = {}

        event = provider.parse_webhook_event(headers, payload)

        assert event.branch == "main"
        assert event.commit_sha == "abc123de"
        assert event.sender == "developer"


class TestWebhookEventParsing:
    """Tests for webhook event object."""

    def test_webhook_event_push_attributes(self):
        """Test WebhookEvent has push attributes."""
        from fraisier.git.base import WebhookEvent

        event = WebhookEvent(
            event_type="push",
            branch="main",
            commit_sha="abc123",
            sender="user",
        )

        assert event.event_type == "push"
        assert event.branch == "main"
        assert event.commit_sha == "abc123"
        assert event.sender == "user"
        assert event.is_push is True
        assert event.is_ping is False

    def test_webhook_event_ping_attributes(self):
        """Test WebhookEvent ping detection."""
        from fraisier.git.base import WebhookEvent

        event = WebhookEvent(event_type="ping")

        assert event.is_ping is True
        assert event.is_push is False

    def test_webhook_event_pull_request_attributes(self):
        """Test WebhookEvent pull_request detection."""
        from fraisier.git.base import WebhookEvent

        event = WebhookEvent(event_type="pull_request")

        assert event.is_pull_request is True
        assert event.is_push is False
