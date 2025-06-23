"""Tests for Docker deployment configurations."""

import json
import subprocess
from pathlib import Path

import pytest
import yaml

from tests.utils.container_utils import requires_any_container, check_container_runtime


class TestDockerfile:
    """Test Dockerfile configurations."""

    def test_dockerfile_exists(self) -> None:
        """Test that Dockerfile exists."""
        dockerfile_path = Path("Dockerfile")
        assert dockerfile_path.exists(), "Dockerfile must exist in project root"

    @requires_any_container
    def test_dockerfile_syntax(self) -> None:
        """Test Dockerfile syntax is valid."""
        runtime = check_container_runtime()
        if runtime is None:
            pytest.skip("No container runtime available")

        # This will be validated when we build the image
        result = subprocess.run(
            [
                runtime,
                "build",
                "--no-cache",
                "-f",
                "Dockerfile",
                "-t",
                "fraiseql-test-syntax",
                ".",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        assert result.returncode == 0, f"Dockerfile syntax error: {result.stderr}"

    def test_dockerfile_best_practices(self) -> None:
        """Test Dockerfile follows best practices."""
        with Path("Dockerfile").open() as f:
            content = f.read()

        # Check for multi-stage build
        assert "FROM" in content, "FROM instruction must be present"
        assert content.count("FROM") >= 2, "Should use multi-stage build"

        # Check for non-root user
        assert "USER" in content, "Should run as non-root user"

        # Check for HEALTHCHECK
        assert "HEALTHCHECK" in content, "Should include health check"

        # Check for proper EXPOSE
        assert "EXPOSE" in content, "Should expose port"

        # Check for .dockerignore usage
        assert Path(".dockerignore").exists(), ".dockerignore file must exist"

    def test_docker_compose_exists(self) -> None:
        """Test that docker-compose files exist."""
        assert Path("docker-compose.yml").exists(), "docker-compose.yml must exist"
        assert Path("docker-compose.prod.yml").exists(), "docker-compose.prod.yml must exist"

    def test_docker_compose_valid(self) -> None:
        """Test docker-compose files are valid YAML."""
        for compose_file in ["docker-compose.yml", "docker-compose.prod.yml"]:
            with Path(compose_file).open() as f:
                data = yaml.safe_load(f)

            assert "services" in data, f"{compose_file} must define services"
            assert "fraiseql" in data["services"], f"{compose_file} must have fraiseql service"

            # Check required services
            if compose_file == "docker-compose.yml":
                assert "postgres" in data["services"], "Development compose must include postgres"

            # Check environment configuration
            fraiseql_service = data["services"]["fraiseql"]
            assert "environment" in fraiseql_service or "env_file" in fraiseql_service

    def test_production_optimizations(self) -> None:
        """Test production Dockerfile optimizations."""
        with Path("Dockerfile").open() as f:
            content = f.read()

        # Check for proper layer caching
        assert "COPY requirements.txt" in content or "COPY pyproject.toml" in content
        
        # Check that dependencies are copied before source code
        if "COPY src/" in content:
            if "COPY requirements.txt" in content:
                assert content.index("COPY requirements.txt") < content.index("COPY src/")
            elif "COPY pyproject.toml" in content:
                assert content.index("COPY pyproject.toml") < content.index("COPY src/")

        # Check for security scanning comment
        assert "hadolint" in content or "# docker run --rm -i hadolint/hadolint" in content


@requires_any_container
class TestDockerBuild:
    """Test Docker image building."""

    @pytest.fixture
    def docker_image(self):
        """Build test Docker image."""
        image_name = "fraiseql-test:latest"

        # Build the image
        result = subprocess.run(
            ["docker", "build", "-t", image_name, "."],
            capture_output=True,
            text=True,
            check=False,
        )

        assert result.returncode == 0, f"Docker build failed: {result.stderr}"

        yield image_name

        # Cleanup
        subprocess.run(["docker", "rmi", image_name], capture_output=True, check=False)

    def test_image_size(self, docker_image) -> None:
        """Test that Docker image size is reasonable."""
        result = subprocess.run(
            ["docker", "images", docker_image, "--format", "{{.Size}}"],
            capture_output=True,
            text=True,
            check=False,
        )

        size_str = result.stdout.strip()
        # Parse size (e.g., "150MB" -> 150)
        size_value = float(size_str.rstrip("GMKB"))

        # Image should be less than 500MB for production
        assert "MB" in size_str, f"Size must be in MB: {size_str}"
        assert size_value < 500, f"Image too large: {size_str}"

    def test_image_labels(self, docker_image) -> None:
        """Test that Docker image has proper labels."""
        result = subprocess.run(
            ["docker", "inspect", docker_image],
            capture_output=True,
            text=True,
            check=False,
        )

        image_data = json.loads(result.stdout)[0]
        labels = image_data["Config"]["Labels"] or {}

        assert "maintainer" in labels or "org.opencontainers.image.authors" in labels
        assert "version" in labels or "org.opencontainers.image.version" in labels

    def test_healthcheck_endpoint(self, docker_image) -> None:
        """Test that health check endpoint works."""
        runtime = check_container_runtime()
        # Start container
        container_name = "fraiseql-health-test"
        subprocess.run(
            [
                runtime,
                "run",
                "-d",
                "--name",
                container_name,
                "-p",
                "8000:8000",
                docker_image,
            ],
            capture_output=True,
            check=False,
        )

        try:
            # Wait for startup
            import time

            time.sleep(5)

            # Check health endpoint
            result = subprocess.run(
                [runtime, "exec", container_name, "curl", "-f", "http://localhost:8000/health"],
                capture_output=True,
                text=True,
                check=False,
            )

            assert result.returncode == 0, "Health check failed"

        finally:
            # Cleanup
            subprocess.run([runtime, "rm", "-f", container_name], capture_output=True, check=False)


class TestDockerSecurity:
    """Test Docker security configurations."""

    def test_no_secrets_in_image(self) -> None:
        """Test that no secrets are included in the image."""
        with Path("Dockerfile").open() as f:
            content = f.read()

        # Check for common secret patterns
        forbidden_patterns = [
            "PASSWORD=",
            "SECRET=",
            "API_KEY=",
            "TOKEN=",
            "DATABASE_URL=postgresql://",
        ]

        for pattern in forbidden_patterns:
            assert pattern not in content, f"Found potential secret: {pattern}"

    def test_security_scanning_setup(self) -> None:
        """Test that security scanning is documented."""
        # Check for security scanning in CI or documentation
        ci_files = list(Path(".github/workflows").glob("*.yml")) + list(
            Path(".github/workflows").glob("*.yaml"),
        )

        has_security_scan = False
        for ci_file in ci_files:
            with Path(ci_file).open() as f:
                content = f.read()
                if "trivy" in content or "snyk" in content or "clair" in content:
                    has_security_scan = True
                    break

        assert (
            has_security_scan or Path("docs/deployment/docker-security.md").exists()
        ), "Should have security scanning in CI or documentation"


class TestDockerCompose:
    """Test Docker Compose configurations."""

    def test_development_compose(self) -> None:
        """Test development docker-compose configuration."""
        with Path("docker-compose.yml").open() as f:
            config = yaml.safe_load(f)

        # Check postgres configuration
        postgres = config["services"]["postgres"]
        assert "POSTGRES_DB" in postgres["environment"]
        assert "POSTGRES_USER" in postgres["environment"]
        assert "volumes" in postgres, "Should persist postgres data"

        # Check fraiseql configuration
        fraiseql = config["services"]["fraiseql"]
        assert "depends_on" in fraiseql
        assert "postgres" in fraiseql["depends_on"]
        assert "DATABASE_URL" in fraiseql["environment"]
        assert "8000:8000" in fraiseql["ports"]

    def test_production_compose(self) -> None:
        """Test production docker-compose configuration."""
        with Path("docker-compose.prod.yml").open() as f:
            config = yaml.safe_load(f)

        fraiseql = config["services"]["fraiseql"]

        # Production should not have postgres (use external DB)
        assert "postgres" not in config["services"], "Production should use external database"

        # Check production settings
        assert "FRAISEQL_PRODUCTION" in fraiseql["environment"]
        assert fraiseql["environment"]["FRAISEQL_PRODUCTION"] == "true"

        # Check resource limits
        assert "deploy" in fraiseql, "Should have deployment configuration"
        assert "resources" in fraiseql["deploy"], "Should have resource limits"

        # Check restart policy
        assert "restart" in fraiseql
        assert fraiseql["restart"] == "unless-stopped"
