"""Tests for webhook handler and FastAPI routes."""

import json
from unittest.mock import MagicMock, patch

import pytest
from fastapi.testclient import TestClient

from fraisier.git import WebhookEvent
from fraisier.webhook import (
    app,
    execute_deployment,
    process_webhook_event,
)


@pytest.fixture
def webhook_client():
    """Create test client for FastAPI app."""
    return TestClient(app)


@pytest.fixture
def sample_webhook_payload():
    """Sample GitHub webhook payload."""
    return {
        "ref": "refs/heads/main",
        "repository": {
            "name": "test-repo",
            "url": "https://github.com/test/test-repo",
        },
        "pusher": {
            "name": "developer",
            "email": "dev@example.com",
        },
        "commits": [
            {
                "id": "abc123def456",
                "message": "Deploy to production",
                "timestamp": "2026-01-22T10:30:00Z",
            }
        ],
    }


@pytest.fixture
def sample_webhook_event():
    """Sample normalized webhook event."""
    return WebhookEvent(
        provider="github",
        event_type="push",
        branch="main",
        commit_sha="abc123def456",
        sender="developer",
        is_push=True,
        is_ping=False,
    )


class TestExecuteDeployment:
    """Tests for execute_deployment background task."""

    @pytest.mark.asyncio
    async def test_execute_deployment_api_success(self, test_db, mock_subprocess):
        """Test successful API deployment via webhook."""
        mock_subprocess.return_value = MagicMock(
            returncode=0,
            stdout="Deployment successful\n",
        )

        fraise_config = {
            "type": "api",
            "app_path": "/tmp/test-api",
            "systemd_service": "test-api.service",
            "health_check": {"url": "http://localhost:8000/health", "timeout": 10},
        }

        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config.return_value._config = {"git": {}}

            await execute_deployment(
                fraise_name="my_api",
                environment="production",
                fraise_config=fraise_config,
                webhook_id=1,
                git_branch="main",
                git_commit="abc123",
            )

            # Verify deployment was recorded
            deployments = test_db.get_recent_deployments(limit=1)
            assert len(deployments) > 0
            assert deployments[0]["fraise_name"] == "my_api"
            assert deployments[0]["environment"] == "production"

    @pytest.mark.asyncio
    async def test_execute_deployment_with_webhook_link(self, test_db):
        """Test that webhook ID is linked to deployment."""
        fraise_config = {
            "type": "api",
            "app_path": "/tmp/test-api",
            "systemd_service": "test-api.service",
            "health_check": {"url": "http://localhost:8000/health", "timeout": 10},
        }

        # Record a webhook event first
        webhook_id = test_db.record_webhook_event(
            event_type="push",
            payload=json.dumps({"test": "payload"}),
            branch="main",
            commit_sha="abc123",
            sender="dev",
            git_provider="github",
        )

        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config.return_value._config = {"git": {}}

            await execute_deployment(
                fraise_name="my_api",
                environment="production",
                fraise_config=fraise_config,
                webhook_id=webhook_id,
                git_branch="main",
                git_commit="abc123",
            )

            # Verify webhook was linked to deployment
            webhooks = test_db.get_recent_webhooks(limit=1)
            assert len(webhooks) > 0
            assert webhooks[0]["processed"] == 1
            assert webhooks[0]["fk_deployment"] is not None

    @pytest.mark.asyncio
    async def test_execute_deployment_etl_type(self, test_db, mock_subprocess):
        """Test ETL deployment via webhook."""
        mock_subprocess.return_value = MagicMock(returncode=0, stdout="ETL ran\n")

        fraise_config = {
            "type": "etl",
            "app_path": "/var/etl",
            "script_path": "scripts/pipeline.py",
        }

        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config.return_value._config = {"git": {}}

            await execute_deployment(
                fraise_name="data_pipeline",
                environment="production",
                fraise_config=fraise_config,
                git_branch="main",
            )

            deployments = test_db.get_recent_deployments(limit=1)
            assert len(deployments) > 0
            assert deployments[0]["fraise_name"] == "data_pipeline"

    @pytest.mark.asyncio
    async def test_execute_deployment_unknown_type_logs_error(self, test_db, caplog):
        """Test that unknown fraise type is logged."""
        fraise_config = {"type": "unknown"}

        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config.return_value._config = {"git": {}}

            await execute_deployment(
                fraise_name="unknown_fraise",
                environment="production",
                fraise_config=fraise_config,
            )

            # Should log error about unknown type
            assert "Unknown fraise type" in caplog.text


class TestProcessWebhookEvent:
    """Tests for process_webhook_event function."""

    def test_process_push_event_with_configured_fraise(self, test_db):
        """Test push event triggers deployment for configured fraise."""
        event = WebhookEvent(
            provider="github",
            event_type="push",
            branch="main",
            commit_sha="abc123",
            sender="dev",
            is_push=True,
            is_ping=False,
        )

        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config_obj = MagicMock()
            mock_config_obj.get_fraise_for_branch.return_value = {
                "fraise_name": "my_api",
                "environment": "production",
                "type": "api",
                "app_path": "/tmp/api",
            }
            mock_config.return_value = mock_config_obj

            from fastapi import BackgroundTasks

            background_tasks = BackgroundTasks()

            result = process_webhook_event(event, background_tasks, webhook_id=1)

            assert result["status"] == "deployment_triggered"
            assert result["fraise"] == "my_api"
            assert result["environment"] == "production"
            assert result["branch"] == "main"
            assert result["provider"] == "github"

    def test_process_push_event_no_configured_fraise(self):
        """Test push event with no configured fraise."""
        event = WebhookEvent(
            provider="gitlab",
            event_type="push",
            branch="feature/xyz",
            commit_sha="xyz789",
            sender="dev",
            is_push=True,
            is_ping=False,
        )

        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config_obj = MagicMock()
            mock_config_obj.get_fraise_for_branch.return_value = None
            mock_config.return_value = mock_config_obj

            from fastapi import BackgroundTasks

            background_tasks = BackgroundTasks()

            result = process_webhook_event(event, background_tasks, webhook_id=1)

            assert result["status"] == "ignored"
            assert "No fraise configured" in result["reason"]
            assert result["provider"] == "gitlab"

    def test_process_ping_event(self):
        """Test ping event returns pong."""
        event = WebhookEvent(
            provider="github",
            event_type="ping",
            branch=None,
            commit_sha=None,
            sender=None,
            is_push=False,
            is_ping=True,
        )

        from fastapi import BackgroundTasks

        background_tasks = BackgroundTasks()

        result = process_webhook_event(event, background_tasks, webhook_id=1)

        assert result["status"] == "pong"
        assert result["provider"] == "github"
        assert "Webhook configured successfully" in result["message"]

    def test_process_other_events_ignored(self):
        """Test that PR events are ignored."""
        event = WebhookEvent(
            provider="github",
            event_type="pull_request",
            branch="feature/new",
            commit_sha="def456",
            sender="dev",
            is_push=False,
            is_ping=False,
        )

        from fastapi import BackgroundTasks

        background_tasks = BackgroundTasks()

        result = process_webhook_event(event, background_tasks, webhook_id=1)

        assert result["status"] == "ignored"
        assert result["event"] == "pull_request"


class TestWebhookRoutes:
    """Tests for FastAPI webhook routes."""

    def test_health_check_endpoint(self, webhook_client):
        """Test /health endpoint."""
        response = webhook_client.get("/health")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "healthy"
        assert data["service"] == "fraisier-webhook"

    def test_list_providers_endpoint(self, webhook_client):
        """Test /providers endpoint."""
        response = webhook_client.get("/providers")
        assert response.status_code == 200
        data = response.json()
        assert "providers" in data
        assert "github" in data["providers"]
        assert "gitlab" in data["providers"]
        assert "gitea" in data["providers"]
        assert "bitbucket" in data["providers"]
        assert "configured" in data

    def test_list_fraises_endpoint(self, webhook_client, sample_config):
        """Test /fraises endpoint."""
        with patch("fraisier.webhook.get_config") as mock_config:
            mock_config.return_value = sample_config

            response = webhook_client.get("/fraises")
            assert response.status_code == 200
            data = response.json()
            assert "fraises" in data
            assert len(data["fraises"]) > 0

    def test_webhook_post_invalid_signature(self, webhook_client, sample_webhook_payload):
        """Test webhook with invalid signature is rejected."""
        response = webhook_client.post(
            "/webhook",
            json=sample_webhook_payload,
            headers={
                "X-GitHub-Event": "push",
                "X-Hub-Signature-256": "sha256=invalid_signature",
            },
        )
        assert response.status_code == 401
        assert "Invalid signature" in response.text

    def test_webhook_post_malformed_json(self, webhook_client):
        """Test webhook with malformed JSON is rejected."""
        response = webhook_client.post(
            "/webhook",
            content=b"not json",
            headers={"X-GitHub-Event": "push"},
        )
        assert response.status_code == 400
        assert "Invalid JSON" in response.text

    def test_webhook_post_unknown_provider(self, webhook_client, sample_webhook_payload):
        """Test webhook with unknown provider."""
        response = webhook_client.post(
            "/webhook?provider=unknown",
            json=sample_webhook_payload,
        )
        assert response.status_code == 400

    def test_webhook_provider_auto_detection_github(self, webhook_client, test_db):
        """Test webhook provider auto-detection for GitHub."""
        with patch("fraisier.webhook.get_provider") as mock_get_provider:
            mock_provider = MagicMock()
            mock_provider.verify_webhook_signature.return_value = True
            mock_provider.parse_webhook_event.return_value = WebhookEvent(
                provider="github",
                event_type="ping",
                branch=None,
                commit_sha=None,
                sender=None,
                is_push=False,
                is_ping=True,
            )
            mock_get_provider.return_value = mock_provider

            response = webhook_client.post(
                "/webhook",
                json={"zen": "test"},
                headers={"X-GitHub-Event": "ping"},
            )

            assert response.status_code == 200
            # Verify provider was detected
            mock_get_provider.assert_called()
            call_args = mock_get_provider.call_args
            assert call_args[0][0] == "github"

    def test_webhook_provider_auto_detection_gitlab(self, webhook_client, test_db):
        """Test webhook provider auto-detection for GitLab."""
        with patch("fraisier.webhook.get_provider") as mock_get_provider:
            mock_provider = MagicMock()
            mock_provider.verify_webhook_signature.return_value = True
            mock_provider.parse_webhook_event.return_value = WebhookEvent(
                provider="gitlab",
                event_type="ping",
                branch=None,
                commit_sha=None,
                sender=None,
                is_push=False,
                is_ping=True,
            )
            mock_get_provider.return_value = mock_provider

            response = webhook_client.post(
                "/webhook",
                json={"hook": "test"},
                headers={"X-Gitlab-Event": "ping"},
            )

            assert response.status_code == 200
            # Verify provider was detected
            mock_get_provider.assert_called()
            call_args = mock_get_provider.call_args
            assert call_args[0][0] == "gitlab"

    def test_webhook_records_event_in_database(self, webhook_client, test_db):
        """Test webhook event is recorded in database."""
        with patch("fraisier.webhook.get_provider") as mock_get_provider:
            mock_provider = MagicMock()
            mock_provider.verify_webhook_signature.return_value = True
            mock_provider.parse_webhook_event.return_value = WebhookEvent(
                provider="github",
                event_type="push",
                branch="main",
                commit_sha="abc123",
                sender="developer",
                is_push=True,
                is_ping=False,
            )
            mock_get_provider.return_value = mock_provider

            payload = {"test": "data"}
            response = webhook_client.post(
                "/webhook",
                json=payload,
                headers={"X-GitHub-Event": "push"},
            )

            assert response.status_code == 200

            # Verify event was recorded
            webhooks = test_db.get_recent_webhooks(limit=1)
            assert len(webhooks) > 0
            assert webhooks[0]["event_type"] == "push"
            assert webhooks[0]["branch_name"] == "main"
            assert webhooks[0]["commit_sha"] == "abc123"
            assert webhooks[0]["sender"] == "developer"
            assert webhooks[0]["git_provider"] == "github"

    def test_github_legacy_endpoint(self, webhook_client, test_db):
        """Test legacy /webhook/github endpoint still works."""
        with patch("fraisier.webhook.get_provider") as mock_get_provider:
            mock_provider = MagicMock()
            mock_provider.verify_webhook_signature.return_value = True
            mock_provider.parse_webhook_event.return_value = WebhookEvent(
                provider="github",
                event_type="ping",
                branch=None,
                commit_sha=None,
                sender=None,
                is_push=False,
                is_ping=True,
            )
            mock_get_provider.return_value = mock_provider

            response = webhook_client.post(
                "/webhook/github",
                json={"zen": "test"},
            )

            assert response.status_code == 200
            assert response.json()["status"] == "pong"


class TestWebhookIntegration:
    """Integration tests for webhook handling."""

    def test_full_webhook_flow_github_push(self, webhook_client, test_db):
        """Test complete flow: webhook → parse → record → deploy."""
        with patch("fraisier.webhook.get_provider") as mock_get_provider:
            with patch("fraisier.webhook.get_config") as mock_get_config:
                # Setup provider mock
                mock_provider = MagicMock()
                mock_provider.verify_webhook_signature.return_value = True
                mock_provider.parse_webhook_event.return_value = WebhookEvent(
                    provider="github",
                    event_type="push",
                    branch="main",
                    commit_sha="abc123def456",
                    sender="developer",
                    is_push=True,
                    is_ping=False,
                )
                mock_get_provider.return_value = mock_provider

                # Setup config mock (no fraise configured for this branch)
                mock_config = MagicMock()
                mock_config.get_fraise_for_branch.return_value = None
                mock_get_config.return_value = mock_config

                payload = {
                    "ref": "refs/heads/main",
                    "repository": {"name": "test-repo"},
                    "pusher": {"name": "developer"},
                }

                response = webhook_client.post(
                    "/webhook",
                    json=payload,
                    headers={"X-GitHub-Event": "push"},
                )

                # Should complete successfully
                assert response.status_code == 200
                data = response.json()
                assert data["status"] == "ignored"  # No fraise configured

                # Verify event was recorded
                webhooks = test_db.get_recent_webhooks(limit=1)
                assert len(webhooks) > 0
                assert webhooks[0]["event_type"] == "push"
                assert webhooks[0]["provider"] == "github"
                assert webhooks[0]["processed"] == 0  # Not linked (no deployment)
