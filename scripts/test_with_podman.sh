#!/bin/bash
# Test runner script for Podman environments

# Detect container runtime
if command -v podman &> /dev/null; then
    echo "Podman detected, configuring environment..."
    export TESTCONTAINERS_PODMAN=true
    export TESTCONTAINERS_RYUK_DISABLED=true
    echo "Environment configured for Podman"
elif command -v docker &> /dev/null; then
    echo "Docker detected, using default configuration"
else
    echo "No container runtime detected (Docker or Podman)"
    echo "Database tests will be skipped"
fi

# Activate virtual environment if it exists
if [ -f ".venv/bin/activate" ]; then
    source .venv/bin/activate
fi

# Run tests
echo "Running tests..."
python -m pytest "$@"
