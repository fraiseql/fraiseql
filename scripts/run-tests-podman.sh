#!/bin/bash
# ABOUTME: Script to run tests in a self-contained podman container
# ABOUTME: Uses Unix socket for PostgreSQL connection inside container

set -e

# Check if image exists, build if not
if ! podman images | grep -q "fraiseql-test-runner"; then
    echo "Building test container..."
    podman build -f Dockerfile.test-all-in-one -t fraiseql-test-runner .
else
    echo "Using existing test container image..."
fi

echo "Running tests in container..."
podman run --rm \
    --name fraiseql-test-run \
    -v "$(pwd)/src:/home/testuser/app/src:ro" \
    -v "$(pwd)/tests:/home/testuser/app/tests:ro" \
    -v "$(pwd)/README.md:/home/testuser/app/README.md:ro" \
    fraiseql-test-runner "$@"
