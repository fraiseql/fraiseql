"""Tests for deployment providers."""

import asyncio
import json
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from fraisier.providers import (
    BareMetalProvider,
    HealthCheck,
    HealthCheckType,
    ProviderType,
)


class TestBareMetalProvider:
    """Test Bare Metal provider implementation."""

    def test_creation_with_host(self):
        """Test creating provider with required host."""
        config = {
            "host": "prod.example.com",
            "username": "deploy",
            "port": 22,
        }
        provider = BareMetalProvider(config)
        assert provider.host == "prod.example.com"
        assert provider.username == "deploy"
        assert provider.port == 22

    def test_creation_without_host_fails(self):
        """Test that provider requires host."""
        config = {"username": "deploy"}
        with pytest.raises(ValueError):
            BareMetalProvider(config)

    def test_default_values(self):
        """Test provider default values."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        assert provider.port == 22
        assert provider.username == "root"

    def test_provider_type(self):
        """Test provider returns correct type."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        assert provider._get_provider_type() == ProviderType.BARE_METAL

    @pytest.mark.asyncio
    async def test_connect_without_asyncssh_fails(self):
        """Test connect fails gracefully if asyncssh not available."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)

        with patch.dict("sys.modules", {"asyncssh": None}):
            with pytest.raises(ConnectionError):
                await provider.connect()

    @pytest.mark.asyncio
    async def test_execute_command_not_connected(self):
        """Test execute_command fails if not connected."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)

        with pytest.raises(RuntimeError):
            await provider.execute_command("ls -la")

    @pytest.mark.asyncio
    @pytest.mark.skip(reason="httpx not installed")
    async def test_health_check_http(self):
        """Test HTTP health check."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        health_check = HealthCheck(
            type=HealthCheckType.HTTP,
            url="http://localhost:8000/health",
            timeout=5,
            retries=1,
        )

        with patch("httpx.AsyncClient") as mock_client:
            mock_response = AsyncMock()
            mock_response.status_code = 200
            mock_client_instance = AsyncMock()
            mock_client_instance.__aenter__ = AsyncMock(return_value=mock_client_instance)
            mock_client_instance.__aexit__ = AsyncMock(return_value=None)
            mock_client_instance.get = AsyncMock(return_value=mock_response)
            mock_client.return_value = mock_client_instance

            result = await provider.check_health(health_check)
            assert result is True

    @pytest.mark.asyncio
    async def test_health_check_tcp(self):
        """Test TCP health check."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        health_check = HealthCheck(
            type=HealthCheckType.TCP,
            port=8000,
            timeout=5,
            retries=1,
        )

        with patch("asyncio.open_connection") as mock_connect:
            mock_reader = AsyncMock()
            mock_writer = AsyncMock()
            mock_writer.wait_closed = AsyncMock()
            mock_connect.return_value = (mock_reader, mock_writer)

            result = await provider.check_health(health_check)
            assert result is True
            mock_writer.close.assert_called_once()

    @pytest.mark.asyncio
    async def test_get_service_status_active(self):
        """Test getting status of active service."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.side_effect = [
                (0, "active", ""),
                (0, "ActiveState=active\nSubState=running", ""),
            ]

            status = await provider.get_service_status("api")
            assert status["service"] == "api"
            assert status["active"] is True

    @pytest.mark.asyncio
    async def test_get_service_status_inactive(self):
        """Test getting status of inactive service."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (3, "", "Unit api.service could not be found")

            status = await provider.get_service_status("api")
            assert status["service"] == "api"
            assert status["active"] is False

    @pytest.mark.asyncio
    async def test_start_service_success(self):
        """Test starting a service."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.start_service("api")
            assert result is True
            mock_exec.assert_called_once()

    @pytest.mark.asyncio
    async def test_restart_service_success(self):
        """Test restarting a service."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.restart_service("api")
            assert result is True

    @pytest.mark.asyncio
    async def test_enable_service_success(self):
        """Test enabling a service."""
        config = {"host": "server.com"}
        provider = BareMetalProvider(config)
        provider.ssh_client = MagicMock()

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.enable_service("api")
            assert result is True


class TestHealthCheck:
    """Test health check configuration."""

    def test_health_check_http_defaults(self):
        """Test HTTP health check defaults."""
        hc = HealthCheck(type=HealthCheckType.HTTP, url="http://localhost:8000")
        assert hc.timeout == 30
        assert hc.retries == 3
        assert hc.retry_delay == 2

    def test_health_check_tcp_config(self):
        """Test TCP health check configuration."""
        hc = HealthCheck(type=HealthCheckType.TCP, port=3306, timeout=10)
        assert hc.port == 3306
        assert hc.timeout == 10

    def test_health_check_exec_config(self):
        """Test exec health check configuration."""
        hc = HealthCheck(
            type=HealthCheckType.EXEC,
            command="curl http://localhost:8000/health",
        )
        assert hc.command == "curl http://localhost:8000/health"

    def test_health_check_systemd_config(self):
        """Test systemd health check configuration."""
        hc = HealthCheck(type=HealthCheckType.SYSTEMD, service="api")
        assert hc.service == "api"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])


class TestDockerComposeProvider:
    """Test Docker Compose provider implementation."""

    def test_creation_with_defaults(self):
        """Test creating provider with default values."""
        from fraisier.providers import DockerComposeProvider

        config = {}
        provider = DockerComposeProvider(config)
        assert provider.compose_file == "docker-compose.yml"
        assert provider.project_name == "fraisier"
        assert provider.timeout == 300

    def test_creation_with_config(self):
        """Test creating provider with custom config."""
        from fraisier.providers import DockerComposeProvider

        config = {
            "compose_file": "docker-compose.prod.yml",
            "project_name": "my_app",
            "timeout": 600,
        }
        provider = DockerComposeProvider(config)
        assert provider.compose_file == "docker-compose.prod.yml"
        assert provider.project_name == "my_app"
        assert provider.timeout == 600

    def test_provider_type(self):
        """Test provider returns correct type."""
        from fraisier.providers import DockerComposeProvider

        config = {}
        provider = DockerComposeProvider(config)
        assert provider._get_provider_type() == ProviderType.DOCKER_COMPOSE

    @pytest.mark.asyncio
    async def test_connect_without_docker_fails(self):
        """Test connect fails if docker not available."""
        from fraisier.providers import DockerComposeProvider

        config = {}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (1, "", "docker: command not found")
            with pytest.raises(ConnectionError):
                await provider.connect()

    @pytest.mark.asyncio
    async def test_get_service_status(self):
        """Test getting service status."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        # Mock the command execution
        mock_output = json.dumps(
            [
                {
                    "ID": "abc123def456",
                    "Image": "nginx:latest",
                    "State": "running",
                }
            ]
        )

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, mock_output, "")

            status = await provider.get_service_status("web")
            assert status["service"] == "web"
            assert status["active"] is True
            assert status["state"] == "running"

    @pytest.mark.asyncio
    async def test_start_service_success(self):
        """Test starting a service."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.start_service("web")
            assert result is True
            mock_exec.assert_called_once()

    @pytest.mark.asyncio
    async def test_stop_service_success(self):
        """Test stopping a service."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.stop_service("web")
            assert result is True

    @pytest.mark.asyncio
    async def test_restart_service_success(self):
        """Test restarting a service."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.restart_service("web")
            assert result is True

    @pytest.mark.asyncio
    async def test_pull_image_success(self):
        """Test pulling latest image."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "Pulling from nginx...", "")

            result = await provider.pull_image("web")
            assert result is True

    @pytest.mark.asyncio
    async def test_get_container_logs(self):
        """Test retrieving container logs."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        mock_logs = "web_1  | nginx started\nweb_1  | listening on port 80"

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, mock_logs, "")

            logs = await provider.get_container_logs("web", lines=50)
            assert "nginx started" in logs

    @pytest.mark.asyncio
    async def test_scale_service(self):
        """Test scaling a service."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.scale_service("api", replicas=3)
            assert result is True

    @pytest.mark.asyncio
    async def test_validate_compose_file_success(self):
        """Test compose file validation."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            result = await provider.validate_compose_file()
            assert result is True

    @pytest.mark.asyncio
    async def test_get_service_env(self):
        """Test getting service environment variables."""
        from fraisier.providers import DockerComposeProvider

        config = {"compose_file": "docker-compose.yml", "project_name": "test"}
        provider = DockerComposeProvider(config)

        mock_env = "PATH=/usr/local/sbin:/usr/local/bin\nDATABASE_URL=postgres://localhost"

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, mock_env, "")

            env = await provider.get_service_env("web")
            assert env["DATABASE_URL"] == "postgres://localhost"

    @pytest.mark.asyncio
    async def test_execute_command_success(self):
        """Test command execution."""
        from fraisier.providers import DockerComposeProvider

        config = {}
        provider = DockerComposeProvider(config)

        with patch("asyncio.create_subprocess_shell") as mock_shell:
            mock_process = AsyncMock()
            mock_process.returncode = 0
            mock_process.communicate = AsyncMock(
                return_value=(b"output", b"")
            )
            mock_shell.return_value = mock_process

            exit_code, stdout, stderr = await provider.execute_command("echo hello")
            assert exit_code == 0
            assert stdout == "output"

    @pytest.mark.asyncio
    async def test_execute_command_timeout(self):
        """Test command timeout."""
        from fraisier.providers import DockerComposeProvider

        config = {}
        provider = DockerComposeProvider(config)

        with patch("asyncio.create_subprocess_shell") as mock_shell:
            mock_process = AsyncMock()
            mock_process.communicate = AsyncMock(
                side_effect=asyncio.TimeoutError()
            )
            mock_process.kill = MagicMock()
            mock_shell.return_value = mock_process

            with pytest.raises(RuntimeError):
                await provider.execute_command("sleep 1000", timeout=1)

    @pytest.mark.asyncio
    async def test_health_check_tcp(self):
        """Test TCP health check for Docker Compose."""
        from fraisier.providers import DockerComposeProvider

        config = {}
        provider = DockerComposeProvider(config)

        health_check = HealthCheck(
            type=HealthCheckType.TCP,
            port=5432,
            timeout=5,
            retries=1,
        )

        with patch("asyncio.open_connection") as mock_connect:
            mock_reader = AsyncMock()
            mock_writer = AsyncMock()
            mock_writer.wait_closed = AsyncMock()
            mock_connect.return_value = (mock_reader, mock_writer)

            result = await provider.check_health(health_check)
            assert result is True
            mock_writer.close.assert_called_once()
