#!/bin/bash
# Setup script for complex domain benchmarking

echo "🚀 Setting up FraiseQL complex domain benchmark environment..."

# Stop any existing containers
echo "📦 Stopping existing containers..."
docker-compose -f docker-compose.yml down
docker-compose -f docker-compose.complex.yml down

# Build the complex domain image
echo "🔨 Building FraiseQL complex domain Docker image..."
docker build -f Dockerfile.complex -t fraiseql-complex .

# Start PostgreSQL first
echo "🌟 Starting PostgreSQL with complex schema..."
docker-compose -f docker-compose.complex.yml up -d postgres-bench

# Wait for PostgreSQL to be ready
echo "⏳ Waiting for PostgreSQL to initialize complex schema..."
sleep 20  # Complex schema takes longer to initialize

# Verify database is ready
echo "🔍 Verifying database initialization..."
docker exec -i postgres-complex-bench psql -U benchmark -d benchmark_db -c "SELECT COUNT(*) as organization_count FROM organizations;"
docker exec -i postgres-complex-bench psql -U benchmark -d benchmark_db -c "SELECT COUNT(*) as employee_count FROM employees;"
docker exec -i postgres-complex-bench psql -U benchmark -d benchmark_db -c "SELECT COUNT(*) as project_count FROM projects;"
docker exec -i postgres-complex-bench psql -U benchmark -d benchmark_db -c "SELECT COUNT(*) as task_count FROM tasks;"

# Start Redis
echo "🎯 Starting Redis..."
docker-compose -f docker-compose.complex.yml up -d redis

# Start the applications
echo "🚀 Starting FraiseQL and Strawberry..."
docker-compose -f docker-compose.complex.yml up -d fraiseql-complex strawberry

# Wait for services to be ready
echo "⏳ Waiting for all services to be ready..."
sleep 10

# Check health
echo "🔍 Checking service health..."
echo ""
echo "FraiseQL Complex:"
curl -s http://localhost:8000/health | jq .
echo ""
echo "Database statistics:"
curl -s http://localhost:8000/benchmark/stats | jq .database_stats

echo ""
echo "✅ Complex domain benchmark environment ready!"
echo ""
echo "Services running:"
echo "  - FraiseQL Complex: http://localhost:8000"
echo "  - Strawberry: http://localhost:8001"
echo "  - PostgreSQL: localhost:5432"
echo "  - Redis: localhost:6379"
echo ""
echo "Run benchmark with:"
echo "  ./benchmark_complex_domain.py"
echo ""
echo "Available endpoints:"
echo "  - Simple: /benchmark/organizations/simple"
echo "  - Complex: /benchmark/organizations/hierarchy"
echo "  - Deep: /benchmark/projects/deep"
echo "  - Full: /benchmark/projects/full-details"
echo "  - Mutations: /benchmark/mutations/*"
