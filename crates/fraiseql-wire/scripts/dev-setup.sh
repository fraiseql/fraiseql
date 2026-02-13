#!/bin/bash

# Development setup script for fraiseql-wire
# This script sets up the development environment with Docker

set -e

echo "🚀 fraiseql-wire Development Setup"
echo "=================================="
echo ""

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is not installed. Please install Docker first."
    exit 1
fi

# Check if docker-compose is available
if ! command -v docker-compose &> /dev/null; then
    echo "❌ docker-compose is not installed. Please install docker-compose first."
    exit 1
fi

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed. Install from https://rustup.rs/"
    exit 1
fi

echo "✓ Docker is installed"
echo "✓ docker-compose is installed"
echo "✓ Rust is installed"
echo ""

# Build Docker image
echo "📦 Building Docker image..."
docker-compose build
echo "✓ Docker image built"
echo ""

# Start PostgreSQL
echo "🐘 Starting PostgreSQL container..."
docker-compose up -d
echo "✓ PostgreSQL container started"
echo ""

# Wait for PostgreSQL to be ready
echo "⏳ Waiting for PostgreSQL to be ready..."
max_attempts=30
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if docker-compose exec -T postgres pg_isready -U postgres > /dev/null 2>&1; then
        echo "✓ PostgreSQL is ready!"
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
    if [ $attempt -eq $max_attempts ]; then
        echo "❌ PostgreSQL failed to start after $max_attempts attempts"
        exit 1
    fi
done
echo ""

# Build Rust project
echo "🦀 Building Rust project..."
cargo build
echo "✓ Rust project built"
echo ""

# Run tests
echo "🧪 Running tests..."
cargo test --lib
echo "✓ Tests passed"
echo ""

# Run integration tests
echo "🔗 Running integration tests..."
cargo test --test integration -- --ignored --nocapture 2>/dev/null || echo "ℹ️  Integration tests require proper setup"
echo ""

echo "=================================="
echo "✅ Development environment is ready!"
echo ""
echo "Quick commands:"
echo "  make test               - Run unit tests"
echo "  make integration-test   - Run integration tests"
echo "  make check              - Run all checks (fmt, clippy, test)"
echo "  make docker-logs        - View PostgreSQL logs"
echo "  make docker-down        - Stop containers"
echo ""
echo "PostgreSQL is running at: localhost:5432"
echo "User: postgres | Password: postgres | Database: fraiseql_test"
