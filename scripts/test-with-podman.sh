#!/bin/bash
# Run tests with Podman container support

echo "🚀 Running FraiseQL tests with Podman support..."

# Set required environment variables
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true

# Optional: Set Podman socket if not default
if [ -S "/run/user/$(id -u)/podman/podman.sock" ]; then
    export DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock"
fi

# Run tests with all arguments passed through
pytest "$@"
