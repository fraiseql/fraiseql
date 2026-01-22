"""Tests for provider implementations and registry.

Tests cover:
- BareMetalProvider: SSH/systemd deployment
- DockerComposeProvider: Docker Compose deployments
- CoolifyProvider: Coolify cloud platform deployments
- CoolifyClient: Coolify API integration
- ProviderRegistry: Plugin management
- DeploymentLock: Concurrency control
"""

from datetime import UTC, datetime, timedelta
from unittest.mock import MagicMock, patch

import pytest
import yaml

from fraisier.deployers.base import DeploymentStatus
from fraisier.locking import DeploymentLock, DeploymentLockedError
from fraisier.providers import ProviderConfig, ProviderRegistry
from fraisier.providers.bare_metal import BareMetalProvider
from fraisier.providers.docker_compose import DockerComposeProvider

# Import coolify modules only if requests is available
try:
    from fraisier.providers.coolify import CoolifyProvider
    from fraisier.providers.coolify_client import (
        CoolifyAPIError,
        CoolifyAuthError,
        CoolifyClient,
        CoolifyNotFoundError,
    )
    HAS_COOLIFY = True
except ImportError:
    HAS_COOLIFY = False


class TestBareMetalProvider:
    """Tests for BareMetalProvider SSH/systemd deployment."""

    @pytest.fixture
    def provider_config(self):
        """Create valid Bare Metal provider configuration."""
        return ProviderConfig(
            name="production",
            type="bare_metal",
            url="prod.example.com",
            custom_fields={
                "ssh_user": "deploy",
                "ssh_key_path": "/home/deploy/.ssh/id_rsa",
                "app_path": "/var/app",
                "systemd_service": "my_api.service",
                "health_check_type": "http",
                "health_check_url": "http://localhost:8000/health",
                "health_check_timeout": 10,
            },
        )

    @pytest.fixture
    def provider(self, provider_config):
        """Create BareMetalProvider instance."""
        return BareMetalProvider(provider_config)

    def test_init_with_valid_config(self, provider, provider_config):
        """Test provider initialization with valid configuration."""
        assert provider.name == "production"
        assert provider.type == "bare_metal"
        assert provider.ssh_host == "prod.example.com"
        assert provider.ssh_user == "deploy"
        assert provider.app_path == "/var/app"
        assert provider.systemd_service == "my_api.service"

    def test_init_missing_ssh_host(self):
        """Test initialization fails if URL (SSH host) is missing."""
        config = ProviderConfig(
            name="test",
            type="bare_metal",
            url=None,
            custom_fields={},
        )

        from fraisier.providers import ProviderConfigError

        with pytest.raises(ProviderConfigError):
            BareMetalProvider(config)

    def test_pre_flight_check_success(self, provider):
        """Test successful SSH connection check."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="SSH connection test\n",
                stderr="",
            )

            success, message = provider.pre_flight_check()

            assert success is True
            assert "SSH connection" in message
            assert "prod.example.com" in message

    def test_pre_flight_check_failure(self, provider):
        """Test failed SSH connection check."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=1,
                stdout="",
                stderr="Permission denied (publickey)",
            )

            success, message = provider.pre_flight_check()

            assert success is False
            assert "SSH connection failed" in message

    def test_pre_flight_check_timeout(self, provider):
        """Test SSH connection timeout."""
        import subprocess

        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = subprocess.TimeoutExpired("ssh", 10)

            success, message = provider.pre_flight_check()

            assert success is False
            assert "timed out" in message

    def test_deploy_service_success(self, provider):
        """Test successful service deployment."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.side_effect = ["abc123", "def456"]  # old, new version
                mock_ssh.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                with patch.object(provider, "health_check", return_value=True):
                    result = provider.deploy_service(
                        service_name="my_api",
                        version="def456",
                        config={"branch": "main"},
                    )

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS
        assert result.old_version == "abc123"
        assert result.new_version == "def456"

    def test_deploy_service_git_pull_fails(self, provider):
        """Test deployment fails when git pull fails."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.return_value = "abc123"
                mock_ssh.return_value = {
                    "returncode": 1,
                    "stdout": "",
                    "stderr": "fatal: not a git repository",
                }

                result = provider.deploy_service(
                    service_name="my_api",
                    version="def456",
                    config={"branch": "main"},
                )

        assert result.success is False
        assert "Git pull failed" in result.error_message

    def test_deploy_service_systemctl_fails(self, provider):
        """Test deployment fails when systemctl restart fails."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.return_value = "abc123"

                # First call (git pull) succeeds, second call (systemctl) fails
                mock_ssh.side_effect = [
                    {"returncode": 0, "stdout": "", "stderr": ""},
                    {"returncode": 1, "stdout": "", "stderr": "Failed to restart service"},
                ]

                result = provider.deploy_service(
                    service_name="my_api",
                    version="def456",
                    config={"branch": "main"},
                )

        assert result.success is False
        assert "Systemctl restart failed" in result.error_message

    def test_deploy_service_health_check_fails(self, provider):
        """Test deployment fails when health check fails."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                with patch.object(provider, "health_check", return_value=False):
                    mock_version.return_value = "abc123"
                    mock_ssh.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                    result = provider.deploy_service(
                        service_name="my_api",
                        version="def456",
                        config={"branch": "main"},
                    )

        assert result.success is False
        assert "Health check failed" in result.error_message

    def test_deploy_service_missing_app_path(self, provider):
        """Test deployment fails if app_path not configured."""
        provider.app_path = None

        result = provider.deploy_service(
            service_name="my_api",
            version="def456",
            config={"branch": "main"},
        )

        assert result.success is False
        assert "app_path not configured" in result.error_message

    def test_deploy_service_missing_systemd_service(self, provider):
        """Test deployment fails if systemd_service not configured."""
        provider.systemd_service = None

        result = provider.deploy_service(
            service_name="my_api",
            version="def456",
            config={"branch": "main"},
        )

        assert result.success is False
        assert "systemd_service not configured" in result.error_message

    def test_get_service_status(self, provider):
        """Test getting service status."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.return_value = "abc123"

                # First call (is-active), second call (show)
                mock_ssh.side_effect = [
                    {"returncode": 0, "stdout": "active\n", "stderr": ""},
                    {"returncode": 0, "stdout": "MainPID=1234\n", "stderr": ""},
                ]

                status = provider.get_service_status("my_api")

        assert status["status"] == "running"
        assert status["version"] == "abc123"
        assert "MainPID" in status["custom"]["stdout"]

    def test_get_service_status_stopped(self, provider):
        """Test status of stopped service."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.return_value = "abc123"
                mock_ssh.return_value = {"returncode": 3, "stdout": "", "stderr": ""}

                status = provider.get_service_status("my_api")

        assert status["status"] == "stopped"

    def test_rollback_service_success(self, provider):
        """Test successful service rollback."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.side_effect = ["abc123", "HEAD~1"]  # old, new version
                mock_ssh.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                with patch.object(provider, "health_check", return_value=True):
                    result = provider.rollback_service("my_api")

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS

    def test_rollback_service_to_specific_version(self, provider):
        """Test rollback to specific version."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.side_effect = ["abc123", "v1.2.3"]
                mock_ssh.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                with patch.object(provider, "health_check", return_value=True):
                    result = provider.rollback_service("my_api", to_version="v1.2.3")

        assert result.success is True
        assert result.new_version == "v1.2.3"

    def test_rollback_service_git_checkout_fails(self, provider):
        """Test rollback fails if git checkout fails."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            with patch.object(provider, "_get_remote_version") as mock_version:
                mock_version.return_value = "abc123"
                mock_ssh.return_value = {
                    "returncode": 1,
                    "stdout": "",
                    "stderr": "error: pathspec 'v1.0.0' did not match any files",
                }

                result = provider.rollback_service("my_api", to_version="v1.0.0")

        assert result.success is False
        assert "Git checkout failed" in result.error_message

    def test_health_check_http_success(self, provider):
        """Test successful HTTP health check."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            # First call (systemctl is-active), second call (curl)
            mock_ssh.side_effect = [
                {"returncode": 0, "stdout": "", "stderr": ""},
                {"returncode": 0, "stdout": "OK", "stderr": ""},
            ]

            result = provider.health_check("my_api")

        assert result is True

    def test_health_check_http_failure(self, provider):
        """Test failed HTTP health check."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            # Systemctl succeeds but curl fails
            mock_ssh.side_effect = [
                {"returncode": 0, "stdout": "", "stderr": ""},
                {"returncode": 1, "stdout": "", "stderr": "Connection refused"},
            ]

            result = provider.health_check("my_api")

        assert result is False

    def test_health_check_tcp(self, provider):
        """Test TCP health check."""
        provider.health_check_type = "tcp"
        provider.health_check_port = 8000

        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.side_effect = [
                {"returncode": 0, "stdout": "", "stderr": ""},
                {"returncode": 0, "stdout": "Connection successful", "stderr": ""},
            ]

            result = provider.health_check("my_api")

        assert result is True

    def test_health_check_service_stopped(self, provider):
        """Test health check fails if service stopped."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.return_value = {"returncode": 3, "stdout": "", "stderr": "inactive"}

            result = provider.health_check("my_api")

        assert result is False

    def test_health_check_none_type(self, provider):
        """Test health check with type='none' (systemd only)."""
        provider.health_check_type = "none"

        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

            result = provider.health_check("my_api")

        assert result is True

    def test_get_logs(self, provider):
        """Test retrieving service logs."""
        expected_logs = (
            "Jan 22 10:30:00 server systemd[1]: Started my_api service.\n"
            "Jan 22 10:30:05 server my_api[1234]: Server listening on 0.0.0.0:8000\n"
        )

        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.return_value = {"returncode": 0, "stdout": expected_logs, "stderr": ""}

            logs = provider.get_logs("my_api", lines=100)

        assert logs == expected_logs
        # Verify journalctl command was called with correct service
        call_args = mock_ssh.call_args[0][0]
        assert "journalctl" in call_args
        assert "my_api.service" in call_args
        assert "-n 100" in call_args

    def test_get_logs_error(self, provider):
        """Test getting logs when journalctl fails."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.return_value = {
                "returncode": 1,
                "stdout": "",
                "stderr": "Unit my_api.service not found",
            }

            logs = provider.get_logs("my_api")

        assert "Unit my_api.service not found" in logs

    def test_run_ssh_command(self, provider):
        """Test SSH command execution."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="command output\n",
                stderr="",
            )

            result = provider._run_ssh_command("echo 'test'")

        assert result["returncode"] == 0
        assert "command output" in result["stdout"]
        # Verify SSH was configured correctly
        call_args = mock_run.call_args[0][0]
        assert "-i" in call_args
        assert provider.ssh_key_path in call_args
        assert f"{provider.ssh_user}@{provider.ssh_host}" in call_args

    def test_get_remote_version(self, provider):
        """Test getting remote git version."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.return_value = {"returncode": 0, "stdout": "abc123def456\n", "stderr": ""}

            version = provider._get_remote_version()

        assert version == "abc123def456"

    def test_get_remote_version_error(self, provider):
        """Test getting remote version when git fails."""
        with patch.object(provider, "_run_ssh_command") as mock_ssh:
            mock_ssh.return_value = {
                "returncode": 128,
                "stdout": "",
                "stderr": "fatal: not a git repository",
            }

            version = provider._get_remote_version()

        assert version is None


class TestProviderRegistry:
    """Tests for ProviderRegistry plugin system."""

    @pytest.fixture(autouse=True)
    def clear_registry(self):
        """Clear registry before each test."""
        ProviderRegistry._providers.clear()
        yield
        ProviderRegistry._providers.clear()

    def test_register_provider(self):
        """Test registering a new provider."""
        ProviderRegistry.register(BareMetalProvider)

        assert ProviderRegistry.is_registered("bare_metal")
        assert "bare_metal" in ProviderRegistry.list_providers()

    def test_register_duplicate_provider(self):
        """Test registering duplicate provider raises error."""
        ProviderRegistry.register(BareMetalProvider)

        with pytest.raises(ValueError, match="already registered"):
            ProviderRegistry.register(BareMetalProvider)

    def test_get_provider(self):
        """Test getting provider instance."""
        ProviderRegistry.register(BareMetalProvider)

        config = ProviderConfig(
            name="test",
            type="bare_metal",
            url="test.example.com",
            custom_fields={},
        )

        provider = ProviderRegistry.get_provider("bare_metal", config)

        assert isinstance(provider, BareMetalProvider)
        assert provider.name == "test"

    def test_get_unknown_provider(self):
        """Test getting unknown provider raises error."""
        with pytest.raises(ValueError, match="Unknown provider"):
            ProviderRegistry.get_provider("unknown", ProviderConfig(
                name="test",
                type="unknown",
                url="test.com",
            ))

    def test_list_providers(self):
        """Test listing registered providers."""
        ProviderRegistry.register(BareMetalProvider)

        providers = ProviderRegistry.list_providers()

        assert "bare_metal" in providers
        assert isinstance(providers, list)
        # List should be sorted
        assert providers == sorted(providers)

    def test_is_registered(self):
        """Test checking if provider is registered."""
        assert not ProviderRegistry.is_registered("bare_metal")

        ProviderRegistry.register(BareMetalProvider)

        assert ProviderRegistry.is_registered("bare_metal")


class TestDeploymentLock:
    """Tests for DeploymentLock concurrency control."""

    @pytest.fixture
    def mock_db(self):
        """Create mock database."""
        with patch("fraisier.locking.get_db") as mock_get_db:
            db = MagicMock()
            mock_get_db.return_value = db
            yield db

    def test_lock_context_manager(self, mock_db):
        """Test lock via context manager."""
        mock_db.get_deployment_lock.return_value = None

        with DeploymentLock("my_api", "production"):
            pass

        # Verify lock was acquired and released
        mock_db.acquire_deployment_lock.assert_called_once()
        mock_db.release_deployment_lock.assert_called_once()

    def test_lock_acquire_fails(self, mock_db):
        """Test lock raises error if already locked."""
        mock_db.get_deployment_lock.return_value = {
            "expires_at": "2099-01-01T00:00:00+00:00"
        }

        with pytest.raises(DeploymentLockedError):
            with DeploymentLock("my_api", "production"):
                pass

    def test_lock_expired(self, mock_db):
        """Test expired lock is removed and new lock acquired."""
        # First call returns expired lock, second call returns None
        expired_time = (datetime.now(UTC) - timedelta(minutes=1)).isoformat()
        mock_db.get_deployment_lock.side_effect = [
            {"expires_at": expired_time},
            None,
        ]

        with DeploymentLock("my_api", "production"):
            pass

        # Verify expired lock was released
        mock_db.release_deployment_lock.assert_called()

    def test_lock_is_locked_check(self, mock_db):
        """Test checking if service is locked."""
        mock_db.get_deployment_lock.return_value = {
            "expires_at": "2099-01-01T00:00:00+00:00"
        }

        is_locked = DeploymentLock.is_locked("my_api", "production")

        assert is_locked is True

    def test_lock_get_info(self, mock_db):
        """Test getting lock information."""
        mock_db.get_deployment_lock.return_value = {
            "locked_at": "2026-01-22T10:30:00+00:00",
            "expires_at": "2026-01-22T10:35:00+00:00",
        }

        info = DeploymentLock.get_lock_info("my_api", "production")

        assert info["service_name"] == "my_api"
        assert info["provider_name"] == "production"
        assert info["locked_at"] == "2026-01-22T10:30:00+00:00"

    def test_lock_clear(self, mock_db):
        """Test forcefully clearing lock."""
        result = DeploymentLock.clear_lock("my_api", "production")

        assert result is True
        mock_db.release_deployment_lock.assert_called_once_with("my_api", "production")

    def test_lock_timeout(self):
        """Test lock timeout configuration."""
        lock = DeploymentLock("my_api", "production", timeout=600)

        assert lock.timeout == 600


class TestDockerComposeProvider:
    """Tests for DockerComposeProvider Docker Compose deployment."""

    @pytest.fixture
    def provider_config(self):
        """Create valid Docker Compose provider configuration."""
        return ProviderConfig(
            name="staging",
            type="docker_compose",
            url="/var/compose",
            custom_fields={
                "compose_file": "docker-compose.yml",
                "service_name": "api",
                "health_check_type": "http",
                "health_check_url": "http://localhost:8000/health",
                "health_check_timeout": 10,
                "health_check_retries": 3,
            },
        )

    @pytest.fixture
    def provider(self, provider_config):
        """Create DockerComposeProvider instance."""
        return DockerComposeProvider(provider_config)

    def test_init_with_valid_config(self, provider, provider_config):
        """Test provider initialization with valid configuration."""
        assert provider.name == "staging"
        assert provider.type == "docker_compose"
        assert provider.compose_dir == "/var/compose"
        assert provider.service_name == "api"
        assert provider.health_check_type == "http"

    def test_init_missing_url(self):
        """Test initialization fails if URL (compose directory) is missing."""
        config = ProviderConfig(
            name="test",
            type="docker_compose",
            url=None,
            custom_fields={"service_name": "api"},
        )

        from fraisier.providers import ProviderConfigError

        with pytest.raises(ProviderConfigError):
            DockerComposeProvider(config)

    def test_init_missing_service_name(self):
        """Test initialization fails if service_name is missing."""
        config = ProviderConfig(
            name="test",
            type="docker_compose",
            url="/var/compose",
            custom_fields={},
        )

        from fraisier.providers import ProviderConfigError

        with pytest.raises(ProviderConfigError):
            DockerComposeProvider(config)

    def test_pre_flight_check_success(self, provider):
        """Test successful pre-flight check."""
        with patch(
            "fraisier.providers.docker_compose.subprocess.run"
        ) as mock_run:
            # Mock docker-compose --version
            version_call = MagicMock(returncode=0, stdout="docker-compose version 1.29.0\n")
            # Mock docker-compose config
            config_call = MagicMock(returncode=0, stdout="config output")

            mock_run.side_effect = [version_call, config_call]

            with patch("pathlib.Path.exists", return_value=True):
                with patch("builtins.open", MagicMock()):
                    with patch("yaml.safe_load", return_value={"services": {"api": {}}}):
                        success, message = provider.pre_flight_check()

        assert success is True
        assert "valid and accessible" in message

    def test_pre_flight_check_docker_compose_not_available(self, provider):
        """Test pre-flight check fails if docker-compose not available."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=127, stderr="not found")

            success, message = provider.pre_flight_check()

        assert success is False
        assert "not available" in message

    def test_pre_flight_check_compose_file_not_found(self, provider):
        """Test pre-flight check fails if compose file doesn't exist."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=0)

            with patch("pathlib.Path.exists", return_value=False):
                success, message = provider.pre_flight_check()

        assert success is False
        assert "not found" in message

    def test_pre_flight_check_invalid_yaml(self, provider):
        """Test pre-flight check fails on invalid YAML."""
        import yaml

        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=0)

            with patch("pathlib.Path.exists", return_value=True):
                with patch("builtins.open", MagicMock()):
                    with patch("yaml.safe_load", side_effect=yaml.YAMLError("bad yaml")):
                        success, message = provider.pre_flight_check()

        assert success is False
        assert "Invalid YAML" in message

    def test_deploy_service_success(self, provider):
        """Test successful service deployment."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                with patch.object(provider, "health_check", return_value=True):
                    mock_version.side_effect = ["old-tag", "new-tag"]
                    mock_cmd.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                    result = provider.deploy_service(
                        service_name="api",
                        version="new-tag",
                        config={},
                    )

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS
        assert result.old_version == "old-tag"
        assert result.new_version == "new-tag"

    def test_deploy_service_pull_fails(self, provider):
        """Test deployment fails when docker-compose pull fails."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                mock_version.return_value = "old-tag"
                mock_cmd.return_value = {
                    "returncode": 1,
                    "stdout": "",
                    "stderr": "failed to pull",
                }

                result = provider.deploy_service(
                    service_name="api",
                    version="new-tag",
                    config={},
                )

        assert result.success is False
        assert "pull failed" in result.error_message

    def test_deploy_service_up_fails(self, provider):
        """Test deployment fails when docker-compose up fails."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                mock_version.return_value = "old-tag"

                # First call (pull) succeeds, second call (up) fails
                mock_cmd.side_effect = [
                    {"returncode": 0, "stdout": "", "stderr": ""},
                    {"returncode": 1, "stdout": "", "stderr": "up failed"},
                ]

                result = provider.deploy_service(
                    service_name="api",
                    version="new-tag",
                    config={},
                )

        assert result.success is False
        assert "up failed" in result.error_message

    def test_deploy_service_health_check_fails(self, provider):
        """Test deployment fails when health check fails."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                with patch.object(provider, "health_check", return_value=False):
                    mock_version.return_value = "old-tag"
                    mock_cmd.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                    result = provider.deploy_service(
                        service_name="api",
                        version="new-tag",
                        config={},
                    )

        assert result.success is False
        assert "Health check failed" in result.error_message

    def test_get_service_status(self, provider):
        """Test getting service status."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                mock_version.return_value = "v1.2.3"
                mock_cmd.return_value = {
                    "returncode": 0,
                    "stdout": (
                        "NAME    IMAGE           STATUS\n"
                        "api     api:v1.2.3      Up 5 minutes\n"
                    ),
                    "stderr": "",
                }

                status = provider.get_service_status("api")

        assert status["status"] == "running"
        assert status["version"] == "v1.2.3"
        assert status["container_id"] is not None

    def test_get_service_status_stopped(self, provider):
        """Test status of stopped service."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": "NAME    IMAGE       STATUS\napi     api:v1.0    Exited\n",
                "stderr": "",
            }

            status = provider.get_service_status("api")

        assert status["status"] == "stopped"

    def test_rollback_service_without_version(self, provider):
        """Test rollback fails if no version specified."""
        with patch.object(provider, "_get_current_version") as mock_version:
            mock_version.return_value = "current-tag"

            result = provider.rollback_service("api")

        assert result.success is False
        assert "requires 'to_version'" in result.error_message

    def test_rollback_service_success(self, provider):
        """Test successful service rollback."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                with patch.object(provider, "health_check", return_value=True):
                    mock_version.side_effect = ["v1.2.3", "v1.2.2"]
                    mock_cmd.return_value = {"returncode": 0, "stdout": "", "stderr": ""}

                    result = provider.rollback_service("api", to_version="v1.2.2")

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS

    def test_rollback_service_pull_fails(self, provider):
        """Test rollback fails when docker-compose pull fails."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            with patch.object(provider, "_get_current_version") as mock_version:
                mock_version.return_value = "v1.2.3"
                mock_cmd.return_value = {
                    "returncode": 1,
                    "stdout": "",
                    "stderr": "pull failed",
                }

                result = provider.rollback_service("api", to_version="v1.2.2")

        assert result.success is False
        assert "pull failed" in result.error_message

    def test_health_check_http_success(self, provider):
        """Test successful HTTP health check."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": "Up 5 minutes",
                "stderr": "",
            }

            with patch("urllib.request.urlopen", MagicMock()):
                result = provider.health_check("api")

        assert result is True

    def test_health_check_http_failure(self, provider):
        """Test failed HTTP health check."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": "Up 5 minutes",
                "stderr": "",
            }

            with patch("urllib.request.urlopen", side_effect=Exception("connection refused")):
                result = provider.health_check("api")

        assert result is False

    def test_health_check_tcp_success(self, provider):
        """Test successful TCP health check."""
        provider.health_check_type = "tcp"
        provider.health_check_port = 8000

        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": "Up 5 minutes",
                "stderr": "",
            }

            with patch("socket.socket") as mock_socket:
                mock_sock_instance = MagicMock()
                mock_sock_instance.connect_ex.return_value = 0
                mock_socket.return_value = mock_sock_instance

                result = provider.health_check("api")

        assert result is True

    def test_health_check_exec_success(self, provider):
        """Test successful exec health check."""
        provider.health_check_type = "exec"
        provider.health_check_exec = "curl -f http://localhost:8000/health"

        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.side_effect = [
                {"returncode": 0, "stdout": "Up", "stderr": ""},  # ps check
                {"returncode": 0, "stdout": "ok", "stderr": ""},  # exec check
            ]

            result = provider.health_check("api")

        assert result is True

    def test_health_check_service_not_running(self, provider):
        """Test health check fails if service not running."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 1,
                "stdout": "",
                "stderr": "Service not found",
            }

            result = provider.health_check("api")

        assert result is False

    def test_get_logs(self, provider):
        """Test retrieving service logs."""
        expected_logs = (
            "api_1  | INFO: Application startup complete [2026-01-22 10:00:00]\n"
            "api_1  | INFO: GET /health status=200\n"
        )

        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": expected_logs,
                "stderr": "",
            }

            logs = provider.get_logs("api", lines=100)

        assert logs == expected_logs
        # Verify logs command was called
        call_args = mock_cmd.call_args[0][0]
        assert "logs" in call_args

    def test_get_logs_error(self, provider):
        """Test getting logs when docker-compose logs fails."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 1,
                "stdout": "",
                "stderr": "Service not found",
            }

            logs = provider.get_logs("api")

        assert "Service not found" in logs

    def test_run_compose_command(self, provider):
        """Test compose command execution."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="output\n",
                stderr="",
            )

            result = provider._run_compose_command(["ps"])

        assert result["returncode"] == 0
        assert "output" in result["stdout"]

    def test_get_current_version_from_running_container(self, provider):
        """Test getting version from running container."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": "NAME  IMAGE         STATUS\napi   api:v1.2.3   Up\n",
                "stderr": "",
            }

            version = provider._get_current_version()

        assert version == "v1.2.3"

    def test_get_current_version_from_compose_file(self, provider):
        """Test getting version from compose file."""
        with patch.object(provider, "_run_compose_command") as mock_cmd:
            mock_cmd.return_value = {
                "returncode": 0,
                "stdout": "",  # Empty ps output
                "stderr": "",
            }

            with patch("builtins.open", MagicMock()):
                with patch("yaml.safe_load", return_value={
                    "services": {"api": {"image": "api:v2.0.0"}}
                }):
                    version = provider._get_current_version()

        assert version == "v2.0.0"

    def test_update_compose_env(self, provider, tmp_path):
        """Test updating environment variables in compose file."""
        # Create a temporary compose file
        compose_file = tmp_path / "docker-compose.yml"
        compose_content = """
services:
  api:
    image: api:latest
    environment:
      DEBUG: "false"
"""
        compose_file.write_text(compose_content)
        provider.compose_path = compose_file

        # Update env
        provider._update_compose_env({"DEBUG": "true"}, "v1.0.0")

        # Verify file was updated
        updated = yaml.safe_load(compose_file.read_text())
        assert updated["services"]["api"]["environment"]["DEBUG"] == "true"
        assert updated["services"]["api"]["environment"]["VERSION"] == "v1.0.0"


@pytest.mark.skipif(not HAS_COOLIFY, reason="requests library not available")
class TestCoolifyClient:
    """Tests for CoolifyClient API integration."""

    @pytest.fixture
    def client(self):
        """Create CoolifyClient instance."""
        return CoolifyClient("https://coolify.example.com", "test_api_key")

    def test_init_with_valid_credentials(self, client):
        """Test client initialization with valid credentials."""
        assert client.base_url == "https://coolify.example.com"
        assert client.api_key == "test_api_key"
        assert client.timeout == 30

    def test_init_missing_base_url(self):
        """Test initialization fails if base_url is missing."""
        with pytest.raises(ValueError):
            CoolifyClient(None, "api_key")

    def test_init_missing_api_key(self):
        """Test initialization fails if api_key is missing."""
        with pytest.raises(ValueError):
            CoolifyClient("https://coolify.example.com", None)

    def test_get_headers(self, client):
        """Test headers include authorization."""
        headers = client._get_headers()

        assert "Authorization" in headers
        assert headers["Authorization"] == "Bearer test_api_key"
        assert headers["Content-Type"] == "application/json"

    def test_health_check_success(self, client):
        """Test successful health check."""
        with patch("requests.Session.get") as mock_get:
            mock_response = MagicMock()
            mock_response.status_code = 200
            mock_get.return_value = mock_response

            result = client.health_check()

        assert result is True

    def test_health_check_failure(self, client):
        """Test health check failure."""
        with patch("requests.Session.get") as mock_get:
            mock_get.side_effect = Exception("Connection refused")

            result = client.health_check()

        assert result is False

    def test_make_request_success(self, client):
        """Test successful API request."""
        with patch.object(client.session, "request") as mock_request:
            mock_response = MagicMock()
            mock_response.status_code = 200
            mock_response.json.return_value = {"id": "app-123", "name": "my_app"}
            mock_request.return_value = mock_response

            result = client._make_request("GET", "v1/applications/app-123")

        assert result["id"] == "app-123"

    def test_make_request_auth_error(self, client):
        """Test request with authentication error."""
        with patch.object(client.session, "request") as mock_request:
            mock_response = MagicMock()
            mock_response.status_code = 401
            mock_request.return_value = mock_response

            with pytest.raises(CoolifyAuthError):
                client._make_request("GET", "v1/applications/app-123")

    def test_make_request_not_found(self, client):
        """Test request for non-existent resource."""
        with patch.object(client.session, "request") as mock_request:
            mock_response = MagicMock()
            mock_response.status_code = 404
            mock_request.return_value = mock_response

            with pytest.raises(CoolifyNotFoundError):
                client._make_request("GET", "v1/applications/not-found")

    def test_get_application(self, client):
        """Test getting application details."""
        with patch.object(client, "_make_request") as mock_request:
            mock_request.return_value = {
                "id": "app-123",
                "name": "my_app",
                "status": "running",
            }

            result = client.get_application("app-123")

        assert result["name"] == "my_app"
        assert result["status"] == "running"

    def test_deploy_application(self, client):
        """Test triggering application deployment."""
        with patch.object(client, "_make_request") as mock_request:
            mock_request.return_value = {
                "id": "deploy-456",
                "status": "in_progress",
            }

            result = client.deploy_application(
                "app-123", {"tag": "v1.0.0"}
            )

        assert result["id"] == "deploy-456"
        mock_request.assert_called_once()

    def test_get_deployment_status(self, client):
        """Test getting deployment status."""
        with patch.object(client, "_make_request") as mock_request:
            mock_request.return_value = {
                "id": "deploy-456",
                "status": "completed",
            }

            result = client.get_deployment_status("app-123", "deploy-456")

        assert result["status"] == "completed"

    def test_get_application_logs(self, client):
        """Test retrieving application logs."""
        expected_logs = "2026-01-22 10:00:00 INFO: App started\n"

        with patch.object(client, "_make_request") as mock_request:
            mock_request.return_value = {"logs": expected_logs}

            result = client.get_application_logs("app-123")

        assert result == expected_logs

    def test_update_application_config(self, client):
        """Test updating application configuration."""
        with patch.object(client, "_make_request") as mock_request:
            mock_request.return_value = {"id": "app-123", "updated": True}

            result = client.update_application_config(
                "app-123", {"env": {"DEBUG": "true"}}
            )

        assert result["updated"] is True

    def test_get_application_status(self, client):
        """Test getting application status."""
        with patch.object(client, "_make_request") as mock_request:
            mock_request.return_value = {
                "status": "running",
                "uptime": 3600,
            }

            result = client.get_application_status("app-123")

        assert result["status"] == "running"


@pytest.mark.skipif(not HAS_COOLIFY, reason="requests library not available")
class TestCoolifyProvider:
    """Tests for CoolifyProvider cloud deployments."""

    @pytest.fixture
    def provider_config(self):
        """Create valid Coolify provider configuration."""
        return ProviderConfig(
            name="production",
            type="coolify",
            url="https://coolify.example.com",
            api_key="coolify_api_key_xyz",
            custom_fields={
                "application_id": "app-uuid-123",
                "project_id": "proj-uuid-456",
                "health_check_type": "status_api",
                "poll_interval": 2,
                "poll_timeout": 60,
            },
        )

    @pytest.fixture
    def provider(self, provider_config):
        """Create CoolifyProvider instance."""
        return CoolifyProvider(provider_config)

    def test_init_with_valid_config(self, provider, provider_config):
        """Test provider initialization with valid configuration."""
        assert provider.name == "production"
        assert provider.type == "coolify"
        assert provider.application_id == "app-uuid-123"
        assert provider.health_check_type == "status_api"

    def test_init_missing_url(self):
        """Test initialization fails if URL is missing."""
        config = ProviderConfig(
            name="test",
            type="coolify",
            url=None,
            api_key="key",
            custom_fields={"application_id": "app-123"},
        )

        from fraisier.providers import ProviderConfigError

        with pytest.raises(ProviderConfigError):
            CoolifyProvider(config)

    def test_init_missing_api_key(self):
        """Test initialization fails if api_key is missing."""
        config = ProviderConfig(
            name="test",
            type="coolify",
            url="https://coolify.example.com",
            api_key=None,
            custom_fields={"application_id": "app-123"},
        )

        from fraisier.providers import ProviderConfigError

        with pytest.raises(ProviderConfigError):
            CoolifyProvider(config)

    def test_init_missing_application_id(self):
        """Test initialization fails if application_id is missing."""
        config = ProviderConfig(
            name="test",
            type="coolify",
            url="https://coolify.example.com",
            api_key="key",
            custom_fields={},
        )

        from fraisier.providers import ProviderConfigError

        with pytest.raises(ProviderConfigError):
            CoolifyProvider(config)

    def test_pre_flight_check_success(self, provider):
        """Test successful pre-flight check."""
        with patch.object(provider.client, "health_check", return_value=True):
            with patch.object(provider.client, "get_application") as mock_get:
                mock_get.return_value = {"id": "app-123", "name": "my_app"}

                success, message = provider.pre_flight_check()

        assert success is True
        assert "my_app" in message

    def test_pre_flight_check_api_not_accessible(self, provider):
        """Test pre-flight check fails if API not accessible."""
        with patch.object(provider.client, "health_check", return_value=False):
            success, message = provider.pre_flight_check()

        assert success is False
        assert "not accessible" in message

    def test_pre_flight_check_app_not_found(self, provider):
        """Test pre-flight check fails if application not found."""
        with patch.object(provider.client, "health_check", return_value=True):
            with patch.object(
                provider.client,
                "get_application",
                side_effect=CoolifyNotFoundError("not found"),
            ):
                success, message = provider.pre_flight_check()

        assert success is False
        assert "not found" in message

    def test_deploy_service_success(self, provider):
        """Test successful service deployment."""
        with patch.object(provider, "_get_current_version") as mock_version:
            with patch.object(provider.client, "deploy_application") as mock_deploy:
                with patch.object(provider, "_poll_deployment_status") as mock_poll:
                    with patch.object(provider, "health_check", return_value=True):
                        mock_version.side_effect = ["v1.0.0", "v2.0.0"]
                        mock_deploy.return_value = {"id": "deploy-123"}
                        mock_poll.return_value = {"success": True}

                        result = provider.deploy_service(
                            "api",
                            "v2.0.0",
                            {},
                        )

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS
        assert result.old_version == "v1.0.0"
        assert result.new_version == "v2.0.0"

    def test_deploy_service_no_deployment_id(self, provider):
        """Test deployment fails if no deployment ID returned."""
        with patch.object(provider, "_get_current_version") as mock_version:
            with patch.object(provider.client, "deploy_application") as mock_deploy:
                mock_version.return_value = "v1.0.0"
                mock_deploy.return_value = {}  # No ID

                result = provider.deploy_service("api", "v2.0.0", {})

        assert result.success is False
        assert "No deployment ID" in result.error_message

    def test_deploy_service_health_check_fails(self, provider):
        """Test deployment fails when health check fails."""
        with patch.object(provider, "_get_current_version") as mock_version:
            with patch.object(provider.client, "deploy_application") as mock_deploy:
                with patch.object(provider, "_poll_deployment_status") as mock_poll:
                    with patch.object(provider, "health_check", return_value=False):
                        mock_version.return_value = "v1.0.0"
                        mock_deploy.return_value = {"id": "deploy-123"}
                        mock_poll.return_value = {"success": True}

                        result = provider.deploy_service(
                            "api",
                            "v2.0.0",
                            {},
                        )

        assert result.success is False
        assert "Health check failed" in result.error_message

    def test_get_service_status(self, provider):
        """Test getting service status."""
        with patch.object(provider.client, "get_application_status") as mock_status:
            with patch.object(provider, "_get_current_version") as mock_version:
                mock_status.return_value = {
                    "status": "running",
                    "uptime": 3600,
                }
                mock_version.return_value = "v2.0.0"

                status = provider.get_service_status("api")

        assert status["status"] == "running"
        assert status["version"] == "v2.0.0"

    def test_rollback_service_without_version(self, provider):
        """Test rollback fails if no version specified."""
        with patch.object(provider, "_get_current_version") as mock_version:
            mock_version.return_value = "v2.0.0"

            result = provider.rollback_service("api")

        assert result.success is False
        assert "requires 'to_version'" in result.error_message

    def test_rollback_service_success(self, provider):
        """Test successful service rollback."""
        with patch.object(provider, "_get_current_version") as mock_version:
            with patch.object(provider.client, "deploy_application") as mock_deploy:
                with patch.object(provider, "_poll_deployment_status") as mock_poll:
                    with patch.object(provider, "health_check", return_value=True):
                        mock_version.side_effect = ["v2.0.0", "v1.0.0"]
                        mock_deploy.return_value = {"id": "deploy-456"}
                        mock_poll.return_value = {"success": True}

                        result = provider.rollback_service(
                            "api", to_version="v1.0.0"
                        )

        assert result.success is True
        assert result.new_version == "v1.0.0"

    def test_health_check_status_api(self, provider):
        """Test health check via status API."""
        with patch.object(provider.client, "get_application_status") as mock_status:
            mock_status.return_value = {"status": "running"}

            result = provider.health_check("api")

        assert result is True

    def test_health_check_status_api_not_running(self, provider):
        """Test health check fails when not running."""
        with patch.object(provider.client, "get_application_status") as mock_status:
            mock_status.return_value = {"status": "stopped"}

            result = provider.health_check("api")

        assert result is False

    def test_health_check_http(self, provider):
        """Test health check via HTTP."""
        provider.health_check_type = "http"
        provider.health_check_url = "http://localhost:8000/health"

        with patch("urllib.request.urlopen", MagicMock()):
            result = provider.health_check("api")

        assert result is True

    def test_health_check_none_type(self, provider):
        """Test health check with type='none'."""
        provider.health_check_type = "none"

        result = provider.health_check("api")

        assert result is True

    def test_get_logs(self, provider):
        """Test retrieving service logs."""
        expected_logs = "2026-01-22 10:00:00 INFO: App started\n"

        with patch.object(provider.client, "get_application_logs") as mock_logs:
            mock_logs.return_value = expected_logs

            logs = provider.get_logs("api", lines=100)

        assert logs == expected_logs

    def test_get_logs_error(self, provider):
        """Test getting logs when API fails."""
        with patch.object(provider.client, "get_application_logs") as mock_logs:
            mock_logs.side_effect = Exception("API error")

            logs = provider.get_logs("api")

        assert "Error retrieving logs" in logs

    def test_poll_deployment_status_success(self, provider):
        """Test deployment polling reaches completion."""
        with patch.object(provider.client, "get_deployment_status") as mock_status:
            mock_status.return_value = {"status": "completed"}

            result = provider._poll_deployment_status("deploy-123", "v2.0.0")

        assert result["success"] is True

    def test_poll_deployment_status_failed(self, provider):
        """Test deployment polling detects failure."""
        with patch.object(provider.client, "get_deployment_status") as mock_status:
            mock_status.return_value = {
                "status": "failed",
                "error_message": "Build failed",
            }

            result = provider._poll_deployment_status("deploy-123", "v2.0.0")

        assert result["success"] is False
        assert "Build failed" in result["error_message"]

    def test_poll_deployment_status_timeout(self, provider):
        """Test deployment polling times out."""
        provider.poll_timeout = 0.1  # Very short timeout
        provider.poll_interval = 0.05

        with patch.object(provider.client, "get_deployment_status") as mock_status:
            mock_status.return_value = {"status": "in_progress"}

            result = provider._poll_deployment_status("deploy-123", "v2.0.0")

        assert result["success"] is False
        assert "timed out" in result["error_message"]

    def test_get_current_version(self, provider):
        """Test getting current version from application."""
        with patch.object(provider.client, "get_application") as mock_get:
            mock_get.return_value = {"id": "app-123", "tag": "v2.0.0"}

            version = provider._get_current_version()

        assert version == "v2.0.0"
