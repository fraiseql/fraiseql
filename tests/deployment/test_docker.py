"""Tests for Docker deployment configurations."""

import json
import subprocess
from pathlib import Path

import pytest
import yaml


class TestDockerfile:
    """Test Dockerfile configurations."""

    def test_dockerfile_exists(self):
        """Test that Dockerfile exists."""
        dockerfile_path = Path("Dockerfile")
        assert dockerfile_path.exists(), "Dockerfile must exist in project root"

    def test_dockerfile_syntax(self):
        """Test Dockerfile syntax is valid."""
        # This will be validated when we build the image
        result = subprocess.run(
            ["docker", "build", "--no-cache", "-f", "Dockerfile", "-t", "fraiseql-test-syntax", "."],
            capture_output=True,
            text=True, check=False,
        )
        assert result.returncode == 0, f"Dockerfile syntax error: {result.stderr}"

    def test_dockerfile_best_practices(self):
        """Test Dockerfile follows best practices."""
        with open("Dockerfile") as f:
            content = f.read()

        # Check for multi-stage build
        assert "FROM" in content and content.count("FROM") >= 2, "Should use multi-stage build"

        # Check for non-root user
        assert "USER" in content, "Should run as non-root user"

        # Check for HEALTHCHECK
        assert "HEALTHCHECK" in content, "Should include health check"

        # Check for proper EXPOSE
        assert "EXPOSE" in content, "Should expose port"

        # Check for .dockerignore usage
        assert Path(".dockerignore").exists(), ".dockerignore file must exist"

    def test_docker_compose_exists(self):
        """Test that docker-compose files exist."""
        assert Path("docker-compose.yml").exists(), "docker-compose.yml must exist"
        assert Path("docker-compose.prod.yml").exists(), "docker-compose.prod.yml must exist"

    def test_docker_compose_valid(self):
        """Test docker-compose files are valid YAML."""
        for compose_file in ["docker-compose.yml", "docker-compose.prod.yml"]:
            with open(compose_file) as f:
                data = yaml.safe_load(f)

            assert "services" in data, f"{compose_file} must define services"
            assert "fraiseql" in data["services"], f"{compose_file} must have fraiseql service"

            # Check required services
            if compose_file == "docker-compose.yml":
                assert "postgres" in data["services"], "Development compose must include postgres"

            # Check environment configuration
            fraiseql_service = data["services"]["fraiseql"]
            assert "environment" in fraiseql_service or "env_file" in fraiseql_service

    def test_production_optimizations(self):
        """Test production Dockerfile optimizations."""
        with open("Dockerfile") as f:
            content = f.read()

        # Check for proper layer caching
        assert "COPY requirements.txt" in content or "COPY pyproject.toml" in content
        assert content.index("COPY requirements") < content.index("COPY src/") if "COPY src/" in content else True

        # Check for security scanning comment
        assert "hadolint" in content or "# docker run --rm -i hadolint/hadolint" in content


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
            text=True, check=False,
        )

        assert result.returncode == 0, f"Docker build failed: {result.stderr}"

        yield image_name

        # Cleanup
        subprocess.run(["docker", "rmi", image_name], capture_output=True, check=False)

    def test_image_size(self, docker_image):
        """Test that Docker image size is reasonable."""
        result = subprocess.run(
            ["docker", "images", docker_image, "--format", "{{.Size}}"],
            capture_output=True,
            text=True, check=False,
        )

        size_str = result.stdout.strip()
        # Parse size (e.g., "150MB" -> 150)
        size_value = float(size_str.rstrip("GMKB"))

        # Image should be less than 500MB for production
        assert "MB" in size_str and size_value < 500, f"Image too large: {size_str}"

    def test_image_labels(self, docker_image):
        """Test that Docker image has proper labels."""
        result = subprocess.run(
            ["docker", "inspect", docker_image],
            capture_output=True,
            text=True, check=False,
        )

        image_data = json.loads(result.stdout)[0]
        labels = image_data["Config"]["Labels"] or {}

        assert "maintainer" in labels or "org.opencontainers.image.authors" in labels
        assert "version" in labels or "org.opencontainers.image.version" in labels

    def test_healthcheck_endpoint(self, docker_image):
        """Test that health check endpoint works."""
        # Start container
        container_name = "fraiseql-health-test"
        subprocess.run([
            "docker", "run", "-d", "--name", container_name,
            "-p", "8000:8000",
            docker_image,
        ], capture_output=True, check=False)

        try:
            # Wait for startup
            import time
            time.sleep(5)

            # Check health endpoint
            result = subprocess.run(
                ["docker", "exec", container_name, "curl", "-f", "http://localhost:8000/health"],
                capture_output=True,
                text=True, check=False,
            )

            assert result.returncode == 0, "Health check failed"

        finally:
            # Cleanup
            subprocess.run(["docker", "rm", "-f", container_name], capture_output=True, check=False)


class TestDockerSecurity:
    """Test Docker security configurations."""

    def test_no_secrets_in_image(self):
        """Test that no secrets are included in the image."""
        with open("Dockerfile") as f:
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

    def test_security_scanning_setup(self):
        """Test that security scanning is documented."""
        # Check for security scanning in CI or documentation
        ci_files = list(Path(".github/workflows").glob("*.yml")) + list(Path(".github/workflows").glob("*.yaml"))

        has_security_scan = False
        for ci_file in ci_files:
            with open(ci_file) as f:
                content = f.read()
                if "trivy" in content or "snyk" in content or "clair" in content:
                    has_security_scan = True
                    break

        assert has_security_scan or Path("docs/deployment/docker-security.md").exists(), \
            "Should have security scanning in CI or documentation"


class TestDockerCompose:
    """Test Docker Compose configurations."""

    def test_development_compose(self):
        """Test development docker-compose configuration."""
        with open("docker-compose.yml") as f:
            config = yaml.safe_load(f)

        # Check postgres configuration
        postgres = config["services"]["postgres"]
        assert "POSTGRES_DB" in postgres["environment"]
        assert "POSTGRES_USER" in postgres["environment"]
        assert "volumes" in postgres, "Should persist postgres data"

        # Check fraiseql configuration
        fraiseql = config["services"]["fraiseql"]
        assert "depends_on" in fraiseql and "postgres" in fraiseql["depends_on"]
        assert "DATABASE_URL" in fraiseql["environment"]
        assert "8000:8000" in fraiseql["ports"]

    def test_production_compose(self):
        """Test production docker-compose configuration."""
        with open("docker-compose.prod.yml") as f:
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
        assert "restart" in fraiseql and fraiseql["restart"] == "unless-stopped"
