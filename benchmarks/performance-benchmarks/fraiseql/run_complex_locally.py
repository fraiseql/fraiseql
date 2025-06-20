#!/usr/bin/env python3
"""
Run the complex domain benchmark locally without Docker.
This script starts the FraiseQL complex app and runs benchmarks against it.
"""

import asyncio
import os
import signal
import subprocess
import sys
import time
from pathlib import Path


def signal_handler(sig, frame):
    print("\n✋ Stopping services...")
    sys.exit(0)


signal.signal(signal.SIGINT, signal_handler)

print("🚀 Starting FraiseQL Complex Domain Benchmark (Local Mode)")
print("=" * 60)

# Check if PostgreSQL is accessible
print("🔍 Checking PostgreSQL connection...")
try:
    import asyncpg

    async def check_db():
        try:
            conn = await asyncpg.connect(
                "postgresql://benchmark:benchmark@localhost:5432/benchmark_db"
            )
            await conn.close()
            return True
        except:
            return False

    if not asyncio.run(check_db()):
        print("❌ PostgreSQL is not accessible at localhost:5432")
        print("   Please ensure PostgreSQL is running with:")
        print("   - Database: benchmark_db")
        print("   - User: benchmark")
        print("   - Password: benchmark")
        sys.exit(1)
    else:
        print("✅ PostgreSQL connection successful")
except ImportError:
    print("❌ asyncpg not installed. Run: pip install asyncpg")
    sys.exit(1)

# Check if Redis is accessible
print("🔍 Checking Redis connection...")
try:
    import redis

    r = redis.Redis(host="localhost", port=6379)
    r.ping()
    print("✅ Redis connection successful")
except:
    print("⚠️  Redis not accessible - will run without caching")

# Start the FraiseQL complex app
print("\n🌟 Starting FraiseQL complex domain app...")
fraiseql_process = subprocess.Popen(
    [sys.executable, "ultra_optimized_complex_app.py"],
    env={
        **os.environ,
        "DATABASE_URL": "postgresql://benchmark:benchmark@localhost:5432/benchmark_db",
        "REDIS_HOST": "localhost",
        "REDIS_PORT": "6379",
    },
)

# Wait for the app to start
print("⏳ Waiting for FraiseQL to initialize...")
time.sleep(5)

# Check if the app is running
import requests

try:
    response = requests.get("http://localhost:8000/health")
    if response.status_code == 200:
        print("✅ FraiseQL is running")
        health_data = response.json()
        print(f"   Performance stats: {health_data.get('performance_monitor', {})}")
    else:
        print("❌ FraiseQL health check failed")
        fraiseql_process.terminate()
        sys.exit(1)
except Exception as e:
    print(f"❌ Could not connect to FraiseQL: {e}")
    fraiseql_process.terminate()
    sys.exit(1)

# Initialize the database with complex schema if needed
print("\n📊 Checking database schema...")
try:
    import asyncpg

    async def check_schema():
        conn = await asyncpg.connect("postgresql://benchmark:benchmark@localhost:5432/benchmark_db")

        # Check if schema exists
        exists = await conn.fetchval(
            "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'benchmark')"
        )

        if not exists:
            print("📦 Creating benchmark schema...")
            sql_path = Path("init-db-complex.sql")
            with sql_path.open() as f:
                schema_sql = f.read()
            await conn.execute(schema_sql)
            print("✅ Complex schema created")
        else:
            # Check table counts
            org_count = await conn.fetchval("SELECT COUNT(*) FROM benchmark.organizations")
            emp_count = await conn.fetchval("SELECT COUNT(*) FROM benchmark.employees")
            proj_count = await conn.fetchval("SELECT COUNT(*) FROM benchmark.projects")
            task_count = await conn.fetchval("SELECT COUNT(*) FROM benchmark.tasks")

            print("✅ Schema exists with data:")
            print(f"   - Organizations: {org_count}")
            print(f"   - Employees: {emp_count}")
            print(f"   - Projects: {proj_count}")
            print(f"   - Tasks: {task_count}")

            if org_count == 0:
                print("⚠️  No data found, initializing...")
                sql_path = Path("init-db-complex.sql")
                with sql_path.open() as f:
                    schema_sql = f.read()
                await conn.execute(schema_sql)
                print("✅ Test data loaded")

        await conn.close()

    asyncio.run(check_schema())
except Exception as e:
    print(f"❌ Database setup failed: {e}")
    fraiseql_process.terminate()
    sys.exit(1)

print("\n" + "=" * 60)
print("🎯 Running Complex Domain Benchmarks")
print("=" * 60)

# Run a few example queries to demonstrate
print("\n1️⃣ Testing Simple Organization Query...")
try:
    response = requests.get("http://localhost:8000/benchmark/organizations/simple?limit=10")
    result = response.json()
    print(f"   ✅ Query time: {result.get('query_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Results: {result.get('result_count', 0)}")
except Exception as e:
    print(f"   ❌ Failed: {e}")

print("\n2️⃣ Testing Complex Organization Hierarchy...")
try:
    response = requests.get("http://localhost:8000/benchmark/organizations/hierarchy?limit=3")
    result = response.json()
    print(f"   ✅ Query time: {result.get('query_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Total time: {result.get('total_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Nesting levels: {result.get('nesting_levels', 0)}")
except Exception as e:
    print(f"   ❌ Failed: {e}")

print("\n3️⃣ Testing Deep Project Query...")
try:
    response = requests.get(
        "http://localhost:8000/benchmark/projects/deep?statuses=planning,in_progress&limit=5"
    )
    result = response.json()
    print(f"   ✅ Query time: {result.get('query_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Results: {result.get('result_count', 0)}")
except Exception as e:
    print(f"   ❌ Failed: {e}")

print("\n4️⃣ Testing Ultra-Complex Project Full Details...")
try:
    response = requests.get("http://localhost:8000/benchmark/projects/full-details?limit=2")
    result = response.json()
    print(f"   ✅ Query time: {result.get('query_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Total time: {result.get('total_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Nesting levels: {result.get('nesting_levels', 0)}")
    print(f"   ✅ Relationships: {len(result.get('relationships_included', []))}")
except Exception as e:
    print(f"   ❌ Failed: {e}")

print("\n5️⃣ Testing Mutation - Create Project...")
try:
    import uuid
    from datetime import date

    response = requests.post(
        "http://localhost:8000/benchmark/mutations/create-project",
        json={
            "name": "Benchmark Test Project",
            "description": "Testing mutation performance",
            "department_id": str(uuid.uuid4()),
            "lead_employee_id": str(uuid.uuid4()),
            "budget": "500000.00",
            "start_date": date.today().isoformat(),
            "end_date": date.today().isoformat(),
        },
    )
    result = response.json()
    print(f"   ✅ Execution time: {result.get('execution_time_ms', 'N/A'):.2f}ms")
    print(f"   ✅ Project ID: {result.get('project_id', 'N/A')}")
    print(f"   ✅ Cache invalidated: {result.get('cache_invalidated', False)}")
except Exception as e:
    print(f"   ❌ Failed: {e}")

# Get final stats
print("\n📊 Performance Statistics:")
try:
    response = requests.get("http://localhost:8000/benchmark/stats")
    stats = response.json()
    perf_stats = stats.get("performance_stats", {})
    db_stats = stats.get("database_stats", {})

    print(f"   Total requests: {perf_stats.get('total_requests', 0)}")
    print(f"   Cache hit rate: {perf_stats.get('cache_hit_rate', 0):.1f}%")
    print(f"   Complex queries: {perf_stats.get('complex_query_count', 0)}")
    print(f"   Mutations: {perf_stats.get('mutation_count', 0)}")

    print("\n   Database size:")
    for table, count in db_stats.items():
        print(f"   - {table}: {count:,}")
except Exception as e:
    print(f"   ❌ Could not get stats: {e}")

print("\n" + "=" * 60)
print("✅ Basic benchmark complete!")
print("\nTo run the full benchmark suite:")
print("   python benchmark_complex_domain.py")
print("\nPress Ctrl+C to stop the FraiseQL server")
print("=" * 60)

# Keep the process running
try:
    fraiseql_process.wait()
except KeyboardInterrupt:
    print("\n✋ Stopping FraiseQL...")
    fraiseql_process.terminate()
    print("👋 Goodbye!")
