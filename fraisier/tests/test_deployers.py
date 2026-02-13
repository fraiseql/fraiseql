"""Tests for deployment implementations."""

import time
from unittest.mock import MagicMock, call, patch

import pytest

from fraisier.deployers.api import APIDeployer
from fraisier.deployers.base import DeploymentResult, DeploymentStatus
from fraisier.deployers.etl import ETLDeployer
from fraisier.deployers.scheduled import ScheduledDeployer


class TestAPIDeployer:
    """Tests for API deployer."""

    def test_init(self):
        """Test APIDeployer initialization."""
        config = {
            "app_path": "/var/www/api",
            "systemd_service": "api.service",
            "git_repo": "https://github.com/test/api.git",
            "health_check": {"url": "http://localhost:8000/health", "timeout": 10},
        }
        deployer = APIDeployer(config)

        assert deployer.app_path == "/var/www/api"
        assert deployer.systemd_service == "api.service"
        assert deployer.git_repo == "https://github.com/test/api.git"
        assert deployer.health_check_url == "http://localhost:8000/health"
        assert deployer.health_check_timeout == 10

    def test_get_current_version_success(self, mock_subprocess):
        """Test getting current deployed version."""
        mock_subprocess.return_value = MagicMock(
            stdout="abc123def456abcd\n", returncode=0
        )

        deployer = APIDeployer({"app_path": "/var/www/api"})
        version = deployer.get_current_version()

        assert version == "abc123de"
        mock_subprocess.assert_called_once()

    def test_get_current_version_failure(self, mock_subprocess):
        """Test getting current version when git fails."""
        from subprocess import CalledProcessError

        mock_subprocess.side_effect = CalledProcessError(1, "git")

        deployer = APIDeployer({"app_path": "/var/www/api"})
        version = deployer.get_current_version()

        assert version is None

    def test_get_latest_version_success(self, mock_subprocess):
        """Test getting latest version from remote."""
        mock_subprocess.return_value = MagicMock(
            stdout="fedcba9876543210\n", returncode=0
        )

        deployer = APIDeployer({"git_repo": "https://github.com/test/api.git"})
        version = deployer.get_latest_version()

        assert version == "fedcba98"
        mock_subprocess.assert_called_once()

    def test_execute_success(self, mock_subprocess, mock_requests):
        """Test successful API deployment."""
        config = {
            "app_path": "/var/www/api",
            "systemd_service": "api.service",
            "health_check": {"url": "http://localhost:8000/health"},
            "database": {"strategy": "apply", "tool": "alembic"},
        }

        deployer = APIDeployer(config)

        # Mock git pull success
        mock_subprocess.return_value = MagicMock(returncode=0, stdout="")

        result = deployer.execute()

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS
        assert result.duration_seconds > 0

    def test_execute_handles_git_pull_failure(self, mock_subprocess):
        """Test deployment fails when git pull fails."""
        from subprocess import CalledProcessError

        config = {
            "app_path": "/var/www/api",
            "systemd_service": "api.service",
        }

        deployer = APIDeployer(config)

        # Mock git pull failure
        mock_subprocess.side_effect = CalledProcessError(1, "git pull")

        result = deployer.execute()

        assert result.success is False
        assert result.status == DeploymentStatus.FAILED
        assert "Deployment failed" in result.error_message or result.error_message

    def test_git_pull_calls_correct_command(self, mock_subprocess):
        """Test git pull uses correct flags."""
        deployer = APIDeployer({"app_path": "/var/www/api"})

        deployer._git_pull()

        mock_subprocess.assert_called_once()
        args, kwargs = mock_subprocess.call_args
        assert args[0] == ["git", "pull", "--ff-only"]
        assert kwargs["cwd"] == "/var/www/api"
        assert kwargs["check"] is True

    def test_restart_service_calls_systemctl(self, mock_subprocess):
        """Test service restart uses correct systemctl command."""
        deployer = APIDeployer({"systemd_service": "api.service"})

        deployer._restart_service()

        mock_subprocess.assert_called_once()
        args, kwargs = mock_subprocess.call_args
        assert args[0] == ["sudo", "systemctl", "restart", "api.service"]

    def test_wait_for_health_success(self, mock_requests):
        """Test health check succeeds."""
        deployer = APIDeployer(
            {"health_check": {"url": "http://localhost:8000/health"}}
        )

        result = deployer._wait_for_health(max_attempts=3, delay=0.1)

        assert result is True
        assert mock_requests.called

    def test_wait_for_health_timeout(self, mock_requests):
        """Test health check timeout."""
        mock_requests.side_effect = Exception("Connection refused")

        deployer = APIDeployer(
            {"health_check": {"url": "http://localhost:8000/health"}}
        )

        result = deployer._wait_for_health(max_attempts=2, delay=0.01)

        assert result is False
        assert mock_requests.call_count >= 2

    def test_run_migrations_with_alembic_apply(self, mock_subprocess):
        """Test migrations with alembic apply strategy."""
        config = {
            "database": {
                "tool": "alembic",
                "strategy": "apply",
            }
        }

        deployer = APIDeployer(config)
        deployer._run_migrations()

        # Should call alembic upgrade head
        calls = mock_subprocess.call_args_list
        assert any("alembic" in str(call) for call in calls)
        assert any("upgrade" in str(call) for call in calls)

    def test_run_migrations_with_alembic_rebuild(self, mock_subprocess):
        """Test migrations with alembic rebuild strategy."""
        config = {
            "database": {
                "tool": "alembic",
                "strategy": "rebuild",
            }
        }

        deployer = APIDeployer(config)
        deployer._run_migrations()

        # Should call alembic downgrade base, then upgrade head
        calls = mock_subprocess.call_args_list
        assert len(calls) >= 2
        assert any("downgrade" in str(call) for call in calls)
        assert any("upgrade" in str(call) for call in calls)

    def test_run_migrations_with_confiture_apply(self, mock_subprocess):
        """Test migrations with confiture apply strategy."""
        config = {
            "database": {
                "tool": "confiture",
                "strategy": "apply",
            }
        }

        deployer = APIDeployer(config)
        deployer._run_migrations()

        # Should call confiture build
        calls = mock_subprocess.call_args_list
        assert any("confiture" in str(call) for call in calls)
        assert any("build" in str(call) for call in calls)

    def test_rollback_to_specific_version(self, mock_subprocess, mock_requests):
        """Test rollback to specific commit."""
        config = {
            "app_path": "/var/www/api",
            "systemd_service": "api.service",
            "health_check": {"url": "http://localhost:8000/health"},
        }

        deployer = APIDeployer(config)
        mock_subprocess.return_value = MagicMock(
            stdout="current_version\n", returncode=0
        )

        result = deployer.rollback(to_version="abc123")

        assert result.success is True
        assert result.status == DeploymentStatus.ROLLED_BACK

        # Should call git checkout
        calls = mock_subprocess.call_args_list
        assert any("checkout" in str(call) for call in calls)

    def test_rollback_to_previous_commit(self, mock_subprocess, mock_requests):
        """Test rollback to previous commit (HEAD~1)."""
        deployer = APIDeployer(
            {
                "app_path": "/var/www/api",
                "systemd_service": "api.service",
            }
        )
        mock_subprocess.return_value = MagicMock(stdout="version\n", returncode=0)

        result = deployer.rollback()

        assert result.success is True
        assert result.status == DeploymentStatus.ROLLED_BACK


class TestETLDeployer:
    """Tests for ETL deployer."""

    def test_init(self):
        """Test ETLDeployer initialization."""
        config = {
            "app_path": "/var/etl",
            "script_path": "scripts/pipeline.py",
        }
        deployer = ETLDeployer(config)

        assert deployer.app_path == "/var/etl"
        assert deployer.script_path == "scripts/pipeline.py"

    def test_get_current_version_from_git(self, mock_subprocess):
        """Test getting version from git repo."""
        mock_subprocess.return_value = MagicMock(
            stdout="abc123def456\n", returncode=0
        )

        deployer = ETLDeployer({"app_path": "/var/etl"})
        version = deployer.get_current_version()

        assert version == "abc123de"

    def test_get_latest_version_same_as_current(self, mock_subprocess):
        """Test that ETL latest version equals current (shared code)."""
        mock_subprocess.return_value = MagicMock(
            stdout="abc123def456\n", returncode=0
        )

        deployer = ETLDeployer({"app_path": "/var/etl"})
        current = deployer.get_current_version()
        latest = deployer.get_latest_version()

        assert current == latest

    def test_execute_success_with_script_verification(self, mock_subprocess):
        """Test ETL deployment verifies script exists."""
        config = {
            "app_path": "/var/etl",
            "script_path": "scripts/pipeline.py",
        }

        deployer = ETLDeployer(config)
        mock_subprocess.return_value = MagicMock(returncode=0)

        result = deployer.execute()

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS

    def test_execute_fails_if_script_missing(self, mock_subprocess):
        """Test ETL deployment fails if script doesn't exist."""
        config = {
            "app_path": "/var/etl",
            "script_path": "scripts/missing.py",
        }

        deployer = ETLDeployer(config)
        # Mock test -f to fail (file not found)
        mock_subprocess.return_value = MagicMock(returncode=1)

        result = deployer.execute()

        assert result.success is False
        assert result.status == DeploymentStatus.FAILED
        assert "not found" in result.error_message.lower()

    def test_rollback_success(self, mock_subprocess):
        """Test ETL rollback using git checkout."""
        deployer = ETLDeployer({"app_path": "/var/etl"})
        mock_subprocess.return_value = MagicMock(stdout="version\n", returncode=0)

        result = deployer.rollback(to_version="abc123")

        assert result.success is True
        assert result.status == DeploymentStatus.ROLLED_BACK

        # Should call git checkout
        calls = mock_subprocess.call_args_list
        assert any("checkout" in str(call) for call in calls)

    def test_rollback_to_previous_without_version(self, mock_subprocess):
        """Test rollback to previous commit."""
        deployer = ETLDeployer({"app_path": "/var/etl"})
        mock_subprocess.return_value = MagicMock(stdout="version\n", returncode=0)

        result = deployer.rollback()

        assert result.success is True
        assert result.status == DeploymentStatus.ROLLED_BACK

        # Should call git checkout HEAD~1
        calls = mock_subprocess.call_args_list
        checkout_calls = [c for c in calls if "checkout" in str(c)]
        assert any("HEAD~1" in str(c) for c in checkout_calls)


class TestScheduledDeployer:
    """Tests for Scheduled deployer."""

    def test_init(self):
        """Test ScheduledDeployer initialization."""
        config = {
            "systemd_timer": "backup.timer",
            "systemd_service": "backup.service",
        }
        deployer = ScheduledDeployer(config)

        assert deployer.systemd_timer == "backup.timer"
        assert deployer.systemd_service == "backup.service"

    def test_get_current_version_shows_timer_state(self, mock_subprocess):
        """Test version shows timer active state."""
        mock_subprocess.return_value = MagicMock(
            stdout="ActiveState=active\n", returncode=0
        )

        deployer = ScheduledDeployer({"systemd_timer": "backup.timer"})
        version = deployer.get_current_version()

        assert version == "timer:active"

    def test_is_deployment_needed_when_timer_inactive(self, mock_subprocess):
        """Test deployment needed when timer is not active."""
        mock_subprocess.return_value = MagicMock(returncode=1)  # inactive

        deployer = ScheduledDeployer({"systemd_timer": "backup.timer"})

        assert deployer.is_deployment_needed() is True

    def test_is_deployment_needed_when_timer_active(self, mock_subprocess):
        """Test deployment not needed when timer is active."""
        mock_subprocess.return_value = MagicMock(returncode=0)  # active

        deployer = ScheduledDeployer({"systemd_timer": "backup.timer"})

        assert deployer.is_deployment_needed() is False

    def test_execute_enables_and_starts_timer(self, mock_subprocess):
        """Test scheduled deployment enables and starts timer."""
        config = {
            "systemd_timer": "backup.timer",
        }

        deployer = ScheduledDeployer(config)
        mock_subprocess.return_value = MagicMock(returncode=0, stdout="timer:active\n")

        result = deployer.execute()

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS

        # Should call enable, start, and daemon-reload
        calls = [str(c) for c in mock_subprocess.call_args_list]
        assert any("enable" in c for c in calls)
        assert any("start" in c for c in calls)
        assert any("daemon-reload" in c for c in calls)

    def test_health_check_returns_true_when_active(self, mock_subprocess):
        """Test health check returns true when timer is active."""
        mock_subprocess.return_value = MagicMock(returncode=0)

        deployer = ScheduledDeployer({"systemd_timer": "backup.timer"})

        assert deployer.health_check() is True

    def test_health_check_returns_false_when_inactive(self, mock_subprocess):
        """Test health check returns false when timer is inactive."""
        mock_subprocess.return_value = MagicMock(returncode=1)

        deployer = ScheduledDeployer({"systemd_timer": "backup.timer"})

        assert deployer.health_check() is False

    def test_rollback_stops_and_disables_timer(self, mock_subprocess):
        """Test rollback stops and disables timer."""
        deployer = ScheduledDeployer({"systemd_timer": "backup.timer"})
        mock_subprocess.return_value = MagicMock(stdout="timer:inactive\n", returncode=0)

        result = deployer.rollback()

        assert result.success is True
        assert result.status == DeploymentStatus.ROLLED_BACK

        # Should call stop and disable
        calls = [str(c) for c in mock_subprocess.call_args_list]
        assert any("stop" in c for c in calls)
        assert any("disable" in c for c in calls)


class TestDeploymentResult:
    """Tests for DeploymentResult dataclass."""

    def test_deployment_result_success(self):
        """Test successful deployment result."""
        result = DeploymentResult(
            success=True,
            status=DeploymentStatus.SUCCESS,
            old_version="v1",
            new_version="v2",
            duration_seconds=10.5,
        )

        assert result.success is True
        assert result.status == DeploymentStatus.SUCCESS
        assert result.old_version == "v1"
        assert result.new_version == "v2"
        assert result.duration_seconds == 10.5
        assert result.error_message is None

    def test_deployment_result_failure(self):
        """Test failed deployment result."""
        result = DeploymentResult(
            success=False,
            status=DeploymentStatus.FAILED,
            error_message="Git pull failed",
        )

        assert result.success is False
        assert result.status == DeploymentStatus.FAILED
        assert result.error_message == "Git pull failed"

    def test_deployment_result_with_details(self):
        """Test deployment result with extra details."""
        details = {"reason": "script timeout", "output": "..."}
        result = DeploymentResult(
            success=False,
            status=DeploymentStatus.FAILED,
            error_message="Deployment timed out",
            details=details,
        )

        assert result.details == details
