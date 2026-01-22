"""Tests for provider implementations and registry.

Tests cover:
- BareMetalProvider: SSH/systemd deployment
- ProviderRegistry: Plugin management
- DeploymentLock: Concurrency control
"""

from datetime import UTC, datetime, timedelta
from unittest.mock import MagicMock, patch

import pytest

from fraisier.deployers.base import DeploymentStatus
from fraisier.locking import DeploymentLock, DeploymentLockedError
from fraisier.providers import ProviderConfig, ProviderRegistry
from fraisier.providers.bare_metal import BareMetalProvider


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
