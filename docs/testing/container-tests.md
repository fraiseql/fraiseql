# Container-based Testing

🚀 **FraiseQL uses a UNIFIED CONTAINER APPROACH** - see [unified-container-testing.md](unified-container-testing.md) for architecture details.

FraiseQL includes integration tests that require a container runtime for running PostgreSQL and other services. We support both Docker and Podman as container runtimes.

## Unified Container System

**Key Innovation**: FraiseQL uses a single PostgreSQL container for the entire test session with socket-based communication, providing:
- 🚀 5-10x faster test execution
- 🔌 Unix domain socket communication (with Podman)
- 🔄 Connection pooling
- 📦 Container caching across test runs

See [unified-container-testing.md](unified-container-testing.md) for full details.

## Container Runtime Requirements

The unified container system requires either Docker or Podman. Tests are automatically skipped if no runtime is available.

### Supported Runtimes

- **Docker**: The traditional container runtime
- **Podman**: A daemonless, rootless container runtime (recommended for better security)

### Checking Runtime Availability

The test suite automatically detects which container runtime is available. Tests that require containers will be skipped with an appropriate message if neither Docker nor Podman is available.

## Running Container Tests

### With Docker

```bash
# Ensure Docker daemon is running
docker info

# Run all tests including container tests
pytest
```

### With Podman

```bash
# Podman doesn't require a daemon
podman info

# Run all tests including container tests
pytest
```

### Skipping Container Tests

If you don't have a container runtime available or want to skip container tests:

```bash
# Skip tests marked as requiring containers
pytest -m "not docker"
```

## Test Categories

### Docker-specific Tests
- `tests/deployment/test_docker.py`: Tests Dockerfile syntax and best practices
- Marked with `@requires_docker` decorator

### Container Integration Tests
- `tests/test_turbo_router_integration.py`: Tests requiring PostgreSQL containers
- Use testcontainers library which works with both Docker and Podman

## Podman Compatibility

While our tests are written using Docker commands, Podman provides Docker compatibility through:

1. **Command aliases**: Podman can be aliased as `docker`
2. **Docker API compatibility**: Podman can expose a Docker-compatible API

To use Podman with Docker compatibility:

```bash
# Create docker alias
alias docker=podman

# Or use podman-docker package (on Fedora/RHEL)
sudo dnf install podman-docker
```

## CI/CD Considerations

In CI/CD environments:

1. **GitHub Actions**: Uses Docker by default
2. **GitLab CI**: Can use Docker or Podman runners
3. **Local Development**: Developers can choose their preferred runtime

## Security Considerations

Podman is recommended for development due to:
- **Rootless operation**: Runs without root privileges
- **Daemonless**: No long-running privileged daemon
- **Better isolation**: Each container runs in its own user namespace

## Troubleshooting

### Permission Denied Errors

With Docker:
```bash
# Add user to docker group (requires logout/login)
sudo usermod -aG docker $USER
```

With Podman:
```bash
# Podman runs rootless by default, no special permissions needed
podman info
```

### Container Tests Skipped

If container tests are being skipped:

1. Check runtime availability:
   ```bash
   docker info  # or podman info
   ```

2. Check for permission issues:
   ```bash
   docker ps  # Should list containers without sudo
   ```

3. For Podman, ensure the podman socket is available:
   ```bash
   systemctl --user status podman.socket
   ```

## Future Improvements

We're considering:
1. Native Podman test support without Docker compatibility layer
2. Container-free integration tests using embedded PostgreSQL
3. Lighter-weight test fixtures for faster test execution