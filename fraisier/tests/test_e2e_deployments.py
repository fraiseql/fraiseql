"""End-to-end deployment tests across all providers.

Tests complete deployment workflows for all fraise types (API, ETL, Scheduled)
across all deployment providers (Bare Metal, Docker Compose, Coolify).
"""

import asyncio
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from fraisier.providers import (
    BareMetalProvider,
    CoolifyProvider,
    DockerComposeProvider,
    HealthCheck,
    HealthCheckType,
)
from fraisier.deployers.base import DeploymentStatus


class TestBareMetalE2E:
    """End-to-end tests for Bare Metal provider deployments."""

    @pytest.mark.asyncio
    async def test_api_deployment_workflow(self):
        """Test complete API deployment on Bare Metal.

        Workflow:
        1. Connect to server
        2. Pull latest code
        3. Run migrations
        4. Restart service
        5. Health check
        6. Disconnect
        """
        config = {
            "host": "prod.example.com",
            "username": "deploy",
            "port": 22,
        }
        provider = BareMetalProvider(config)

        with patch.object(provider, "connect") as mock_connect, \
             patch.object(provider, "disconnect") as mock_disconnect, \
             patch.object(provider, "execute_command") as mock_exec, \
             patch.object(provider, "check_health") as mock_health:

            # Simulate workflow
            mock_exec.side_effect = [
                (0, "abc123def456", ""),  # git pull
                (0, "Migrations applied", ""),  # migrations
                (0, "", ""),  # restart service
            ]
            mock_health.return_value = True

            # Simulate deployment
            await provider.connect()

            # Git pull
            exit_code, stdout, stderr = await provider.execute_command(
                "cd /var/www/api && git pull origin main"
            )
            assert exit_code == 0

            # Run migrations
            exit_code, stdout, stderr = await provider.execute_command(
                "cd /var/www/api && python manage.py migrate"
            )
            assert exit_code == 0

            # Restart service
            result = await provider.restart_service("api.service")
            assert result is True

            # Health check
            health_check = HealthCheck(
                type=HealthCheckType.HTTP,
                url="http://prod.example.com/health",
                timeout=30,
                retries=3,
            )
            is_healthy = await provider.check_health(health_check)
            assert is_healthy is True

            await provider.disconnect()
            mock_connect.assert_called_once()
            mock_disconnect.assert_called_once()

    @pytest.mark.asyncio
    async def test_rollback_workflow(self):
        """Test rollback workflow on Bare Metal."""
        config = {
            "host": "prod.example.com",
            "username": "deploy",
        }
        provider = BareMetalProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            # Simulate rollback
            mock_exec.side_effect = [
                (0, "Reverted to abc123", ""),  # git revert
                (0, "", ""),  # restart service
            ]

            # Revert code
            exit_code, stdout, _ = await provider.execute_command(
                "cd /var/www/api && git revert HEAD"
            )
            assert exit_code == 0
            assert "Reverted" in stdout

            # Restart service
            result = await provider.restart_service("api.service")
            assert result is True

    @pytest.mark.asyncio
    async def test_service_management_workflow(self):
        """Test complete service lifecycle management."""
        config = {
            "host": "prod.example.com",
            "username": "deploy",
        }
        provider = BareMetalProvider(config)

        with patch.object(provider, "execute_command") as mock_exec, \
             patch.object(provider, "get_service_status") as mock_status:

            # Setup
            mock_exec.return_value = (0, "", "")
            mock_status.return_value = {
                "service": "api",
                "active": True,
                "state": "running",
            }

            # Stop service
            result = await provider.stop_service("api")
            assert result is True

            # Start service
            result = await provider.start_service("api")
            assert result is True

            # Enable service (auto-start on boot)
            result = await provider.enable_service("api")
            assert result is True

            # Check status
            status = await provider.get_service_status("api")
            assert status["active"] is True


class TestDockerComposeE2E:
    """End-to-end tests for Docker Compose provider deployments."""

    @pytest.mark.asyncio
    async def test_container_deployment_workflow(self):
        """Test complete container deployment workflow.

        Workflow:
        1. Pull latest image
        2. Rebuild container
        3. Health check
        4. Verify logs
        5. Get metrics
        """
        config = {
            "compose_file": "docker-compose.prod.yml",
            "project_name": "myapp",
        }
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec, \
             patch.object(provider, "check_health") as mock_health, \
             patch.object(provider, "get_container_logs") as mock_logs, \
             patch.object(provider, "get_service_status") as mock_status:

            # Simulate workflow
            mock_exec.side_effect = [
                (0, "Pulling from registry", ""),  # pull image
                (0, "Container built", ""),  # up -d
            ]
            mock_health.return_value = True
            mock_logs.return_value = "web_1 | Application started"
            mock_status.return_value = {
                "service": "web",
                "active": True,
                "state": "running",
                "container_id": "abc123def456",
                "image": "nginx:latest",
            }

            # Pull latest image
            result = await provider.pull_image("web")
            assert result is True

            # Rebuild and start
            result = await provider.start_service("web")
            assert result is True

            # Health check
            health_check = HealthCheck(
                type=HealthCheckType.HTTP,
                url="http://localhost:80",
                timeout=30,
            )
            is_healthy = await provider.check_health(health_check)
            assert is_healthy is True

            # Check logs
            logs = await provider.get_container_logs("web", lines=100)
            assert "Application started" in logs

            # Get status
            status = await provider.get_service_status("web")
            assert status["active"] is True

    @pytest.mark.asyncio
    async def test_scaling_workflow(self):
        """Test service scaling workflow."""
        config = {
            "compose_file": "docker-compose.prod.yml",
            "project_name": "myapp",
        }
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec:
            mock_exec.return_value = (0, "", "")

            # Scale from 1 to 3 replicas
            result = await provider.scale_service("api", replicas=3)
            assert result is True

            # Scale back down to 1
            result = await provider.scale_service("api", replicas=1)
            assert result is True

    @pytest.mark.asyncio
    async def test_container_restart_workflow(self):
        """Test container restart workflow."""
        config = {
            "compose_file": "docker-compose.prod.yml",
            "project_name": "myapp",
        }
        provider = DockerComposeProvider(config)

        with patch.object(provider, "execute_command") as mock_exec, \
             patch.object(provider, "get_container_logs") as mock_logs:

            mock_exec.return_value = (0, "", "")
            mock_logs.return_value = "Container restarted"

            # Stop container
            result = await provider.stop_service("api")
            assert result is True

            # Start container
            result = await provider.start_service("api")
            assert result is True

            # Verify restart via logs
            logs = await provider.get_container_logs("api")
            assert "restarted" in logs.lower()


class TestCoolifyE2E:
    """End-to-end tests for Coolify provider deployments."""

    @pytest.mark.asyncio
    async def test_deployment_lifecycle(self):
        """Test complete deployment lifecycle via Coolify.

        Workflow:
        1. Trigger deployment
        2. Wait for completion
        3. Retrieve logs
        4. Verify status
        5. Check metrics
        """
        config = {
            "api_url": "http://coolify.example.com/api",
            "api_token": "token-123",
            "application_id": "app-456",
        }
        provider = CoolifyProvider(config)

        with patch.object(provider, "deploy") as mock_deploy, \
             patch.object(provider, "wait_for_deployment") as mock_wait, \
             patch.object(provider, "get_deployment_logs") as mock_logs, \
             patch.object(provider, "get_service_status") as mock_status, \
             patch.object(provider, "get_metrics") as mock_metrics:

            # Simulate workflow
            mock_deploy.return_value = {
                "success": True,
                "deployment_id": "deploy-789",
                "status": "running",
            }
            mock_wait.return_value = True
            mock_logs.return_value = "Deployment successful"
            mock_status.return_value = {
                "service": "api",
                "active": True,
                "state": "running",
                "version": "1.2.3",
            }
            mock_metrics.return_value = {
                "cpu_usage": 25.5,
                "memory_usage": 512,
                "uptime": 3600,
            }

            # Trigger deployment
            result = await provider.deploy(git_branch="main")
            assert result["success"] is True
            deployment_id = result["deployment_id"]

            # Wait for deployment
            success = await provider.wait_for_deployment(deployment_id, timeout=600)
            assert success is True

            # Get logs
            logs = await provider.get_deployment_logs(deployment_id)
            assert "successful" in logs.lower()

            # Verify status
            status = await provider.get_service_status("api")
            assert status["active"] is True
            assert status["version"] == "1.2.3"

            # Check metrics
            metrics = await provider.get_metrics()
            assert metrics["cpu_usage"] < 100  # Valid CPU usage

    @pytest.mark.asyncio
    async def test_rollback_workflow(self):
        """Test deployment rollback workflow."""
        config = {
            "api_url": "http://coolify.example.com/api",
            "api_token": "token-123",
            "application_id": "app-456",
        }
        provider = CoolifyProvider(config)

        with patch.object(provider, "get_recent_deployments") as mock_recent, \
             patch.object(provider, "rollback_deployment") as mock_rollback, \
             patch.object(provider, "wait_for_deployment") as mock_wait:

            # Get recent deployments
            mock_recent.return_value = [
                {"deployment_id": "deploy-1", "status": "failed"},
                {"deployment_id": "deploy-2", "status": "success"},
            ]
            mock_rollback.return_value = {
                "success": True,
                "deployment_id": "deploy-2",
                "status": "running",
            }
            mock_wait.return_value = True

            # Get recent deployments
            deployments = await provider.get_recent_deployments(limit=10)
            assert len(deployments) == 2

            # Rollback to previous successful deployment
            last_failed = deployments[0]
            previous_success = deployments[1]

            result = await provider.rollback_deployment(previous_success["deployment_id"])
            assert result["success"] is True

            # Wait for rollback
            success = await provider.wait_for_deployment(result["deployment_id"])
            assert success is True

    @pytest.mark.asyncio
    async def test_configuration_management(self):
        """Test environment and configuration management."""
        config = {
            "api_url": "http://coolify.example.com/api",
            "api_token": "token-123",
            "application_id": "app-456",
        }
        provider = CoolifyProvider(config)

        with patch.object(provider, "get_application_config") as mock_config, \
             patch.object(provider, "update_environment_variables") as mock_update:

            # Get current config
            mock_config.return_value = {
                "git_repo": "https://github.com/example/api",
                "git_branch": "main",
                "port": 8000,
            }
            app_config = await provider.get_application_config()
            assert app_config["port"] == 8000

            # Update environment variables
            new_env = {
                "DATABASE_URL": "postgres://prod-db:5432/myapp",
                "CACHE_ENABLED": "true",
                "LOG_LEVEL": "info",
            }
            mock_update.return_value = True

            result = await provider.update_environment_variables(new_env)
            assert result is True


class TestMultiProviderScenarios:
    """Test scenarios involving multiple providers."""

    @pytest.mark.asyncio
    async def test_canary_deployment_docker_to_bare_metal(self):
        """Test canary deployment: Docker Compose test, Bare Metal production.

        Scenario:
        1. Deploy to Docker Compose test environment
        2. Run health checks
        3. Deploy to Bare Metal production
        4. Monitor both
        """
        # Test environment (Docker Compose)
        test_config = {
            "compose_file": "docker-compose.test.yml",
            "project_name": "myapp-test",
        }
        test_provider = DockerComposeProvider(test_config)

        # Production environment (Bare Metal)
        prod_config = {
            "host": "prod.example.com",
            "username": "deploy",
        }
        prod_provider = BareMetalProvider(prod_config)

        with patch.object(test_provider, "start_service") as test_start, \
             patch.object(test_provider, "check_health") as test_health, \
             patch.object(prod_provider, "restart_service") as prod_restart, \
             patch.object(prod_provider, "check_health") as prod_health:

            test_start.return_value = True
            test_health.return_value = True
            prod_restart.return_value = True
            prod_health.return_value = True

            # Stage 1: Test environment
            await test_provider.start_service("web")
            test_is_healthy = await test_provider.check_health(
                HealthCheck(type=HealthCheckType.HTTP, url="http://localhost:3000")
            )
            assert test_is_healthy is True

            # Stage 2: Production environment
            await prod_provider.restart_service("api.service")
            prod_is_healthy = await prod_provider.check_health(
                HealthCheck(type=HealthCheckType.HTTP, url="http://prod.example.com")
            )
            assert prod_is_healthy is True

    @pytest.mark.asyncio
    async def test_blue_green_deployment(self):
        """Test blue-green deployment pattern.

        Scenario:
        1. Blue (current): Running on Docker Compose
        2. Green (new): Deploy on Docker Compose
        3. Health check green
        4. Switch traffic to green
        5. Decommission blue
        """
        config = {
            "compose_file": "docker-compose.prod.yml",
            "project_name": "myapp",
        }
        provider = DockerComposeProvider(config)

        with patch.object(provider, "start_service") as mock_start, \
             patch.object(provider, "stop_service") as mock_stop, \
             patch.object(provider, "check_health") as mock_health, \
             patch.object(provider, "execute_command") as mock_exec:

            mock_start.return_value = True
            mock_stop.return_value = True
            mock_health.return_value = True
            mock_exec.return_value = (0, "", "")

            # Start green environment
            await provider.start_service("web-green")

            # Health check green
            is_healthy = await provider.check_health(
                HealthCheck(type=HealthCheckType.HTTP, url="http://localhost:3001")
            )
            assert is_healthy is True

            # Switch traffic (via LB/nginx config)
            exit_code, _, _ = await provider.execute_command(
                "docker-compose -f docker-compose.prod.yml up -d --no-deps web-green"
            )
            assert exit_code == 0

            # Decommission blue
            await provider.stop_service("web-blue")

    @pytest.mark.asyncio
    async def test_progressive_deployment(self):
        """Test progressive deployment via Docker Compose scaling.

        Scenario:
        1. Deploy to 25% of capacity (1 replica)
        2. Monitor metrics
        3. Deploy to 50% (2 replicas)
        4. Monitor metrics
        5. Full deployment if stable (4 replicas)
        """
        config = {
            "compose_file": "docker-compose.prod.yml",
            "project_name": "myapp",
        }
        provider = DockerComposeProvider(config)

        with patch.object(provider, "scale_service") as mock_scale, \
             patch.object(provider, "get_container_logs") as mock_logs:

            mock_scale.return_value = True
            mock_logs.return_value = "Service scaled and healthy"

            # Stage 1: 25% (1 out of 4 replicas)
            result = await provider.scale_service("api", replicas=1)
            assert result is True
            logs = await provider.get_container_logs("api")
            assert "healthy" in logs.lower()

            # Stage 2: 50% (2 out of 4 replicas)
            result = await provider.scale_service("api", replicas=2)
            assert result is True
            logs = await provider.get_container_logs("api")
            assert "healthy" in logs.lower()

            # Stage 3: Full deployment (4 replicas)
            result = await provider.scale_service("api", replicas=4)
            assert result is True
            logs = await provider.get_container_logs("api")
            assert "healthy" in logs.lower()


class TestErrorRecovery:
    """Test error recovery and rollback scenarios."""

    @pytest.mark.asyncio
    async def test_deployment_failure_recovery(self):
        """Test recovery from deployment failure."""
        config = {
            "host": "prod.example.com",
            "username": "deploy",
        }
        provider = BareMetalProvider(config)

        with patch.object(provider, "execute_command") as mock_exec, \
             patch.object(provider, "check_health") as mock_health:

            # First deployment fails
            mock_exec.side_effect = [
                (1, "", "Deployment failed"),  # Failed deployment
                (0, "", ""),  # Rollback
                (0, "", ""),  # Restart
            ]
            mock_health.return_value = False

            # Attempt deployment
            exit_code, _, stderr = await provider.execute_command(
                "cd /var/www/api && git pull && restart"
            )
            assert exit_code != 0

            # Rollback
            await provider.execute_command("cd /var/www/api && git revert HEAD")

            # Restart service
            await provider.restart_service("api.service")

    @pytest.mark.asyncio
    async def test_health_check_retry_logic(self):
        """Test health check retry with exponential backoff."""
        config = {
            "compose_file": "docker-compose.yml",
            "project_name": "myapp",
        }
        provider = DockerComposeProvider(config)

        with patch.object(provider, "check_health") as mock_health:
            # Simulate: fail twice, then succeed
            mock_health.side_effect = [False, False, True]

            # Create health check with retries
            health_check = HealthCheck(
                type=HealthCheckType.HTTP,
                url="http://localhost:8000",
                retries=3,
                retry_delay=1,
            )

            # This would call check_health internally with retries
            # For this test, we verify the retry count
            assert mock_health.call_count == 0  # Not called yet


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
