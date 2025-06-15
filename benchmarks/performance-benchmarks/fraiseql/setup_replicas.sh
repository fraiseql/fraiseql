#!/bin/bash
# Setup script for read replicas and Nginx load balancing

echo "🚀 Setting up FraiseQL with read replicas and Nginx load balancing..."

# Stop existing containers
echo "📦 Stopping existing containers..."
docker-compose -f docker-compose.yml down
docker-compose -f docker-compose.replicas.yml down

# Build the new ultra-optimized image with replicas
echo "🔨 Building ultra-optimized Docker image with replica support..."
docker build -f Dockerfile.ultra.replicas -t fraiseql-ultra-replicas .

# Start the replica infrastructure
echo "🌟 Starting PostgreSQL primary and replicas..."
docker-compose -f docker-compose.replicas.yml up -d postgres-primary postgres-replica1 postgres-replica2

# Wait for primary to be ready
echo "⏳ Waiting for PostgreSQL primary to be ready..."
sleep 10

# Initialize the database with optimized schema
echo "📊 Initializing database with projection tables..."
docker exec -i postgres-primary psql -U benchmark -d benchmark_db < init-db-ultra.sql

# Wait for replication to catch up
echo "🔄 Waiting for replicas to sync..."
sleep 5

# Start PgPool and Redis
echo "🎯 Starting PgPool and Redis..."
docker-compose -f docker-compose.replicas.yml up -d pgpool redis

# Wait for PgPool to be ready
echo "⏳ Waiting for PgPool to initialize..."
sleep 10

# Start the FraiseQL application with Nginx
echo "🚀 Starting FraiseQL with Nginx load balancing..."
docker-compose -f docker-compose.replicas.yml up -d fraiseql-ultra-replicas

# Wait for everything to be ready
echo "⏳ Waiting for all services to be ready..."
sleep 15

# Check health
echo "🔍 Checking service health..."
curl -s http://localhost:8000/health | jq .

echo "✅ Setup complete!"
echo ""
echo "Services running:"
echo "  - FraiseQL with Nginx: http://localhost:8000"
echo "  - PostgreSQL Primary: localhost:5432"
echo "  - PgPool (load balancer): localhost:5433"
echo "  - Redis: localhost:6379"
echo ""
echo "Monitor with:"
echo "  - Replica stats: curl http://localhost:8000/replica/stats | jq ."
echo "  - Pool stats: curl http://localhost:8000/pools/stats | jq ."
echo "  - Nginx status: curl http://localhost:8000/nginx-status"
