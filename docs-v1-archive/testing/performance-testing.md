# Performance Testing

Performance testing ensures your FraiseQL application meets response time, throughput, and resource usage requirements under various load conditions.

## Testing Strategy

### Performance Test Types

1. **Response Time Testing** - Measure individual query/mutation execution times
2. **Load Testing** - Test performance under expected user load
3. **Stress Testing** - Find breaking points under extreme load
4. **Spike Testing** - Test handling of sudden load increases
5. **Volume Testing** - Test with large amounts of data
6. **Endurance Testing** - Test stability over extended periods

### Performance Metrics

- **Response Time**: Time from request to response (latency)
- **Throughput**: Requests processed per second
- **Error Rate**: Percentage of failed requests
- **Resource Usage**: CPU, memory, database connections
- **Database Performance**: Query execution time, connection pool utilization

## Response Time Testing

### Basic Response Time Tests

```python
# test_response_times.py
import pytest
import time
import asyncio
import statistics
from httpx import AsyncClient

@pytest.mark.asyncio
class TestResponseTimes:
    async def test_user_query_response_time(self, test_client, sample_users):
        """Test users query response time"""
        query = """
            query GetUsers($limit: Int) {
                users(limit: $limit) {
                    id
                    name
                    email
                    createdAt
                }
            }
        """

        # Warm up - first request might be slower due to cold start
        await test_client.post("/graphql", json={
            "query": query,
            "variables": {"limit": 10}
        })

        # Measure multiple requests
        times = []
        for _ in range(50):
            start = time.perf_counter()
            response = await test_client.post("/graphql", json={
                "query": query,
                "variables": {"limit": 10}
            })
            elapsed = time.perf_counter() - start
            times.append(elapsed)

            assert response.status_code == 200
            data = response.json()
            assert "errors" not in data

        # Calculate statistics
        avg_time = statistics.mean(times)
        median_time = statistics.median(times)
        p95_time = statistics.quantiles(times, n=20)[18]  # 95th percentile
        p99_time = statistics.quantiles(times, n=100)[98]  # 99th percentile

        # Performance assertions (adjust thresholds as needed)
        assert avg_time < 0.1, f"Average response time too slow: {avg_time*1000:.2f}ms"
        assert p95_time < 0.2, f"95th percentile too slow: {p95_time*1000:.2f}ms"
        assert p99_time < 0.5, f"99th percentile too slow: {p99_time*1000:.2f}ms"

        # Report metrics
        print(f"\nResponse Time Metrics:")
        print(f"Average: {avg_time*1000:.2f}ms")
        print(f"Median: {median_time*1000:.2f}ms")
        print(f"P95: {p95_time*1000:.2f}ms")
        print(f"P99: {p99_time*1000:.2f}ms")

    async def test_complex_nested_query_performance(self, test_client, sample_users_with_posts):
        """Test performance of complex nested queries"""
        query = """
            query ComplexQuery {
                users(limit: 20) {
                    id
                    name
                    email
                    posts {
                        id
                        title
                        content
                        comments {
                            id
                            content
                            author {
                                name
                            }
                        }
                        createdAt
                    }
                }
            }
        """

        # Measure complex query
        start = time.perf_counter()
        response = await test_client.post("/graphql", json={"query": query})
        elapsed = time.perf_counter() - start

        assert response.status_code == 200
        data = response.json()
        assert "errors" not in data

        # Complex queries should still be reasonably fast
        assert elapsed < 2.0, f"Complex query too slow: {elapsed*1000:.2f}ms"

        # Verify data completeness
        users = data["data"]["users"]
        assert len(users) <= 20

        # Check that nested data was loaded
        users_with_posts = [u for u in users if len(u["posts"]) > 0]
        if users_with_posts:
            sample_user = users_with_posts[0]
            assert "comments" in sample_user["posts"][0]

    async def test_mutation_response_time(self, test_client):
        """Test mutation response times"""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    id
                    name
                    email
                    createdAt
                }
            }
        """

        times = []
        for i in range(20):
            start = time.perf_counter()
            response = await test_client.post("/graphql", json={
                "query": mutation,
                "variables": {
                    "input": {
                        "name": f"Performance User {i}",
                        "email": f"perf{i}@example.com",
                        "password": "password123"
                    }
                }
            })
            elapsed = time.perf_counter() - start
            times.append(elapsed)

            assert response.status_code == 200

        avg_time = statistics.mean(times)
        assert avg_time < 0.2, f"Mutation too slow: {avg_time*1000:.2f}ms"
```

### Database Query Performance

```python
# test_database_performance.py
import pytest
import time
import asyncio

@pytest.mark.asyncio
class TestDatabasePerformance:
    async def test_repository_query_performance(self, test_db):
        """Test raw repository query performance"""
        # Test simple query
        start = time.perf_counter()
        users = await test_db.find("v_user", limit=100)
        elapsed = time.perf_counter() - start

        assert elapsed < 0.1, f"Simple query too slow: {elapsed*1000:.2f}ms"
        assert isinstance(users, list)

    async def test_database_function_performance(self, test_db):
        """Test database function call performance"""
        times = []

        for i in range(10):
            start = time.perf_counter()
            user_id = await test_db.call_function(
                "fn_create_user",
                p_name=f"DB Perf User {i}",
                p_email=f"dbperf{i}@example.com",
                p_password_hash="hash"
            )
            elapsed = time.perf_counter() - start
            times.append(elapsed)

            assert user_id is not None

        avg_time = statistics.mean(times)
        assert avg_time < 0.05, f"DB function too slow: {avg_time*1000:.2f}ms"

    async def test_bulk_operations_performance(self, test_db):
        """Test bulk database operations"""
        # Test bulk insert performance
        start = time.perf_counter()

        # Use asyncio.gather for concurrent inserts
        tasks = []
        for i in range(100):
            task = test_db.call_function(
                "fn_create_user",
                p_name=f"Bulk User {i}",
                p_email=f"bulk{i}@example.com",
                p_password_hash="hash"
            )
            tasks.append(task)

        user_ids = await asyncio.gather(*tasks)
        elapsed = time.perf_counter() - start

        assert len(user_ids) == 100
        assert elapsed < 5.0, f"Bulk insert too slow: {elapsed:.2f}s"

        rate = 100 / elapsed
        print(f"Bulk insert rate: {rate:.1f} users/second")

    async def test_query_with_large_dataset(self, test_db):
        """Test query performance with large dataset"""
        # First ensure we have enough data
        existing_count = await test_db.fetchval("SELECT COUNT(*) FROM tb_user")
        if existing_count < 1000:
            pytest.skip("Large dataset test requires at least 1000 users")

        # Test query performance on large dataset
        start = time.perf_counter()
        users = await test_db.find(
            "v_user",
            where={"name": {"ilike": "%User%"}},
            limit=50
        )
        elapsed = time.perf_counter() - start

        assert len(users) <= 50
        assert elapsed < 0.5, f"Large dataset query too slow: {elapsed*1000:.2f}ms"
```

## Load Testing with Locust

### Basic Load Test Setup

```python
# locustfile.py
from locust import HttpUser, task, between
import random
import json

class FraiseQLUser(HttpUser):
    wait_time = between(1, 3)  # Wait 1-3 seconds between requests

    def on_start(self):
        """Called when a user starts"""
        # Optionally authenticate
        self.login()

    def login(self):
        """Login and get authentication token"""
        response = self.client.post("/graphql", json={
            "query": """
                mutation Login($email: String!, $password: String!) {
                    login(email: $email, password: $password) {
                        token
                        user { id }
                    }
                }
            """,
            "variables": {
                "email": "test@example.com",
                "password": "password123"
            }
        })

        if response.status_code == 200:
            data = response.json()
            if "errors" not in data:
                self.token = data["data"]["login"]["token"]
                self.headers = {"Authorization": f"Bearer {self.token}"}
            else:
                self.headers = {}
        else:
            self.headers = {}

    @task(3)  # Weight: 3 (most common operation)
    def query_users(self):
        """Query users list"""
        self.client.post("/graphql",
            headers=getattr(self, 'headers', {}),
            json={
                "query": """
                    query GetUsers($limit: Int) {
                        users(limit: $limit) {
                            id
                            name
                            email
                        }
                    }
                """,
                "variables": {"limit": 20}
            },
            name="query_users"
        )

    @task(2)  # Weight: 2 (common operation)
    def query_single_user(self):
        """Query specific user with posts"""
        # Use a random user ID (you might need to maintain a list)
        user_id = f"user-{random.randint(1, 1000)}"

        self.client.post("/graphql",
            headers=getattr(self, 'headers', {}),
            json={
                "query": """
                    query GetUser($id: ID!) {
                        user(id: $id) {
                            id
                            name
                            email
                            posts {
                                id
                                title
                            }
                        }
                    }
                """,
                "variables": {"id": user_id}
            },
            name="query_single_user"
        )

    @task(1)  # Weight: 1 (less common operation)
    def create_user(self):
        """Create a new user"""
        user_num = random.randint(10000, 99999)

        self.client.post("/graphql",
            headers=getattr(self, 'headers', {}),
            json={
                "query": """
                    mutation CreateUser($input: CreateUserInput!) {
                        createUser(input: $input) {
                            id
                            name
                            email
                        }
                    }
                """,
                "variables": {
                    "input": {
                        "name": f"Load Test User {user_num}",
                        "email": f"loadtest{user_num}@example.com",
                        "password": "loadtest123"
                    }
                }
            },
            name="create_user"
        )

    @task(1)  # Weight: 1 (less common operation)
    def create_post(self):
        """Create a new post"""
        if hasattr(self, 'token'):  # Only if authenticated
            post_num = random.randint(1000, 9999)

            self.client.post("/graphql",
                headers=self.headers,
                json={
                    "query": """
                        mutation CreatePost($input: CreatePostInput!) {
                            createPost(input: $input) {
                                id
                                title
                            }
                        }
                    """,
                    "variables": {
                        "input": {
                            "title": f"Load Test Post {post_num}",
                            "content": f"This is content for load test post {post_num}",
                            "status": "PUBLISHED"
                        }
                    }
                },
                name="create_post"
            )
```

### Advanced Load Testing

```python
# advanced_locustfile.py
from locust import HttpUser, task, between, events
import random
import time
import logging

class AdvancedFraiseQLUser(HttpUser):
    wait_time = between(0.5, 2)

    def on_start(self):
        """Initialize user session"""
        self.user_id = None
        self.posts = []
        self.setup_user()

    def setup_user(self):
        """Create a user for this test session"""
        response = self.client.post("/graphql", json={
            "query": """
                mutation CreateUser($input: CreateUserInput!) {
                    createUser(input: $input) {
                        id
                        name
                        email
                    }
                }
            """,
            "variables": {
                "input": {
                    "name": f"Session User {random.randint(10000, 99999)}",
                    "email": f"session{random.randint(10000, 99999)}@test.com",
                    "password": "sessionpass"
                }
            }
        })

        if response.status_code == 200:
            data = response.json()
            if "errors" not in data:
                self.user_id = data["data"]["createUser"]["id"]

    @task(5)
    def read_heavy_operations(self):
        """Simulate read-heavy workload"""
        operations = [
            self.query_users,
            self.query_posts,
            self.search_users
        ]

        operation = random.choice(operations)
        operation()

    @task(2)
    def write_operations(self):
        """Simulate write operations"""
        if self.user_id:
            if random.random() < 0.7:  # 70% posts, 30% user updates
                self.create_post()
            else:
                self.update_user()

    def query_users(self):
        """Query users with various filters"""
        filters = [
            {"limit": 20},
            {"limit": 10, "nameContains": "Test"},
            {"limit": 50}
        ]

        variables = random.choice(filters)

        start_time = time.time()
        response = self.client.post("/graphql", json={
            "query": """
                query GetUsers($limit: Int, $nameContains: String) {
                    users(limit: $limit, nameContains: $nameContains) {
                        id
                        name
                        email
                        createdAt
                    }
                }
            """,
            "variables": variables
        })

        # Custom metric tracking
        elapsed = time.time() - start_time
        if response.status_code == 200:
            events.request_success.fire(
                request_type="GraphQL",
                name="query_users",
                response_time=elapsed * 1000,
                response_length=len(response.content)
            )
        else:
            events.request_failure.fire(
                request_type="GraphQL",
                name="query_users",
                response_time=elapsed * 1000,
                exception=f"HTTP {response.status_code}"
            )

    def query_posts(self):
        """Query posts with nested data"""
        self.client.post("/graphql", json={
            "query": """
                query GetPosts($limit: Int) {
                    posts(limit: $limit) {
                        id
                        title
                        author {
                            name
                        }
                        comments {
                            id
                            content
                        }
                    }
                }
            """,
            "variables": {"limit": 15}
        }, name="query_posts")

    def search_users(self):
        """Search users by various criteria"""
        search_terms = ["Test", "User", "Load", "Session"]
        term = random.choice(search_terms)

        self.client.post("/graphql", json={
            "query": """
                query SearchUsers($search: String) {
                    users(nameContains: $search, limit: 10) {
                        id
                        name
                        posts {
                            id
                            title
                        }
                    }
                }
            """,
            "variables": {"search": term}
        }, name="search_users")

    def create_post(self):
        """Create post for authenticated user"""
        if not self.user_id:
            return

        post_titles = [
            "Load Testing Adventures",
            "Performance Matters",
            "GraphQL at Scale",
            "Database Optimization",
            "API Design Principles"
        ]

        title = f"{random.choice(post_titles)} {random.randint(1000, 9999)}"

        response = self.client.post("/graphql", json={
            "query": """
                mutation CreatePost($input: CreatePostInput!) {
                    createPost(input: $input) {
                        id
                        title
                    }
                }
            """,
            "variables": {
                "input": {
                    "title": title,
                    "content": f"Content for {title}",
                    "status": "PUBLISHED"
                }
            }
        }, name="create_post")

        if response.status_code == 200:
            data = response.json()
            if "errors" not in data:
                post_id = data["data"]["createPost"]["id"]
                self.posts.append(post_id)

    def update_user(self):
        """Update user information"""
        if not self.user_id:
            return

        names = ["Updated User", "Modified Name", "Changed User"]
        new_name = f"{random.choice(names)} {random.randint(100, 999)}"

        self.client.post("/graphql", json={
            "query": """
                mutation UpdateUser($id: ID!, $input: UpdateUserInput!) {
                    updateUser(id: $id, input: $input) {
                        id
                        name
                    }
                }
            """,
            "variables": {
                "id": self.user_id,
                "input": {"name": new_name}
            }
        }, name="update_user")

# Custom event handlers for detailed metrics
@events.request_success.add_listener
def on_request_success(request_type, name, response_time, response_length, **kwargs):
    """Log successful requests"""
    if response_time > 1000:  # Log slow requests (>1 second)
        logging.warning(f"Slow request: {name} took {response_time:.2f}ms")

@events.request_failure.add_listener
def on_request_failure(request_type, name, response_time, exception, **kwargs):
    """Log failed requests"""
    logging.error(f"Request failed: {name} - {exception}")
```

### Running Load Tests

```bash
# Basic load test
locust -f locustfile.py --host=http://localhost:8000

# Command line load test (no web UI)
locust -f locustfile.py --host=http://localhost:8000 \
  --users 50 --spawn-rate 5 --run-time 300s --headless

# Distributed load testing
# Master node
locust -f locustfile.py --master --host=http://localhost:8000

# Worker nodes (run multiple)
locust -f locustfile.py --worker --master-host=localhost

# Export results
locust -f locustfile.py --host=http://localhost:8000 \
  --users 100 --spawn-rate 10 --run-time 600s --headless \
  --html=load_test_report.html --csv=load_test_results
```

## N+1 Query Detection

### Automated N+1 Detection

```python
# test_n_plus_one_detection.py
import pytest
import logging
from unittest.mock import patch
import re

@pytest.mark.asyncio
class TestNPlusOneDetection:
    async def test_users_with_posts_no_n_plus_one(self, test_client, sample_users_with_posts):
        """Test that querying users with posts doesn't cause N+1 queries"""

        # Capture database queries
        query_log = []

        def log_query(query, *args):
            query_log.append(query)

        # Mock database execute to capture queries
        with patch('fraiseql.repository.FraiseQLRepository.execute') as mock_execute:
            mock_execute.side_effect = lambda q, *args: log_query(q, *args)

            query = """
                query UsersWithPosts {
                    users(limit: 10) {
                        id
                        name
                        posts {
                            id
                            title
                        }
                    }
                }
            """

            response = await test_client.post("/graphql", json={"query": query})
            assert response.status_code == 200

            # Analyze queries
            select_queries = [q for q in query_log if q.strip().upper().startswith('SELECT')]

            # Should not have more than a reasonable number of queries
            # With proper DataLoader, should be ~2-3 queries total
            assert len(select_queries) <= 5, f"Too many queries detected: {len(select_queries)}"

    async def test_detect_n_plus_one_without_dataloader(self, test_db):
        """Test N+1 detection in raw database operations"""
        # Create test data
        user_ids = []
        for i in range(10):
            user_id = await test_db.call_function(
                "fn_create_user",
                p_name=f"N+1 User {i}",
                p_email=f"nplus1_{i}@test.com",
                p_password_hash="hash"
            )
            user_ids.append(user_id)

            # Create posts for each user
            for j in range(3):
                await test_db.call_function(
                    "fn_create_post",
                    p_title=f"Post {j} by User {i}",
                    p_content=f"Content {j}",
                    p_author_id=user_id
                )

        # Simulate N+1 pattern (bad approach)
        query_count = 0

        async def count_queries():
            nonlocal query_count
            query_count += 1

        # This would be N+1 pattern
        users = await test_db.find("v_user", limit=10)
        await count_queries()  # 1 query

        for user in users:
            # This would cause N additional queries (N+1 problem)
            posts = await test_db.find("v_post", where={"author_id": user["id"]})
            await count_queries()  # N more queries

        # Should detect the N+1 pattern
        expected_queries = 1 + len(users)  # 1 + N
        assert query_count == expected_queries
        assert query_count > 5, "N+1 pattern should result in many queries"

    async def test_optimized_query_pattern(self, test_db):
        """Test optimized query pattern that avoids N+1"""
        # Better approach: single query with JOIN or batch loading
        query = """
            SELECT
                u.id as user_id,
                u.name as user_name,
                u.email as user_email,
                p.id as post_id,
                p.title as post_title,
                p.content as post_content
            FROM tb_user u
            LEFT JOIN tb_post p ON u.id = p.author_id
            WHERE u.id IN (
                SELECT id FROM tb_user LIMIT 10
            )
            ORDER BY u.id, p.created_at
        """

        results = await test_db.execute(query)

        # Single query retrieves all needed data
        # Process results to group by user
        users_with_posts = {}
        for row in results:
            user_id = row["user_id"]
            if user_id not in users_with_posts:
                users_with_posts[user_id] = {
                    "id": user_id,
                    "name": row["user_name"],
                    "email": row["user_email"],
                    "posts": []
                }

            if row["post_id"]:
                users_with_posts[user_id]["posts"].append({
                    "id": row["post_id"],
                    "title": row["post_title"],
                    "content": row["post_content"]
                })

        # Verify we got data efficiently
        assert len(users_with_posts) <= 10
        # Only 1 query executed, avoiding N+1
```

## Memory and Resource Usage Testing

```python
# test_resource_usage.py
import pytest
import psutil
import asyncio
import gc
from memory_profiler import profile

@pytest.mark.asyncio
class TestResourceUsage:
    async def test_memory_usage_under_load(self, test_client):
        """Test memory usage doesn't grow excessively under load"""
        process = psutil.Process()
        initial_memory = process.memory_info().rss / 1024 / 1024  # MB

        # Generate load
        tasks = []
        for i in range(100):
            task = test_client.post("/graphql", json={
                "query": """
                    query GetUsers {
                        users(limit: 50) {
                            id
                            name
                            posts {
                                id
                                title
                            }
                        }
                    }
                """
            })
            tasks.append(task)

        responses = await asyncio.gather(*tasks)

        # Force garbage collection
        gc.collect()

        final_memory = process.memory_info().rss / 1024 / 1024  # MB
        memory_growth = final_memory - initial_memory

        # Memory growth should be reasonable
        assert memory_growth < 100, f"Excessive memory growth: {memory_growth:.2f}MB"

        # Verify all requests succeeded
        for response in responses:
            assert response.status_code == 200

    async def test_connection_pool_usage(self, test_db):
        """Test database connection pool doesn't leak connections"""
        initial_connections = await test_db.fetchval(
            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database()"
        )

        # Create load that uses many connections
        async def db_operation(index):
            await test_db.call_function(
                "fn_create_user",
                p_name=f"Pool Test User {index}",
                p_email=f"pool{index}@test.com",
                p_password_hash="hash"
            )
            return index

        # Run many concurrent operations
        tasks = [db_operation(i) for i in range(50)]
        await asyncio.gather(*tasks)

        # Wait a bit for connections to be returned to pool
        await asyncio.sleep(1)

        final_connections = await test_db.fetchval(
            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database()"
        )

        # Connection count should not have grown significantly
        connection_growth = final_connections - initial_connections
        assert connection_growth <= 5, f"Connection leak detected: {connection_growth} new connections"

    @profile  # Requires memory_profiler: pip install memory-profiler
    async def test_memory_profile_large_query(self, test_client, large_dataset):
        """Profile memory usage of large query results"""
        query = """
            query LargeDataset {
                users(limit: 1000) {
                    id
                    name
                    email
                    posts {
                        id
                        title
                        content
                        comments {
                            id
                            content
                        }
                    }
                }
            }
        """

        response = await test_client.post("/graphql", json={"query": query})
        assert response.status_code == 200

        data = response.json()
        assert "errors" not in data

        # The @profile decorator will output memory usage line by line
```

## Continuous Performance Monitoring

### Performance Test in CI/CD

```yaml
# .github/workflows/performance.yml
name: Performance Tests

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  performance:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: test
          POSTGRES_DB: fraiseql_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:

          - 5432:5432

    steps:

    - uses: actions/checkout@v4

    - name: Set up Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.13'

    - name: Install dependencies
      run: |
        pip install -e ".[dev]"
        pip install locust memory-profiler

    - name: Setup test data
      env:
        TEST_DATABASE_URL: postgresql://postgres:test@localhost/fraiseql_test
      run: |
        python scripts/setup_performance_data.py

    - name: Run performance tests
      env:
        TEST_DATABASE_URL: postgresql://postgres:test@localhost/fraiseql_test
      run: |
        pytest tests/performance/ -v --benchmark-json=benchmark.json

    - name: Run load test
      run: |
        # Start the application in background
        python -m uvicorn app:app --host 0.0.0.0 --port 8000 &
        sleep 10  # Wait for startup

        # Run load test
        locust -f tests/performance/locustfile.py \
          --host=http://localhost:8000 \
          --users 50 --spawn-rate 5 --run-time 60s \
          --headless --csv=load_test_results

    - name: Performance regression check
      run: |
        python scripts/check_performance_regression.py \
          --benchmark-file benchmark.json \
          --baseline-file baseline_performance.json \
          --threshold 20  # 20% regression threshold

    - name: Upload performance results
      uses: actions/upload-artifact@v3
      with:
        name: performance-results
        path: |
          benchmark.json
          load_test_results_*.csv
```

### Performance Baseline Script

```python
# scripts/check_performance_regression.py
import json
import argparse
import sys

def check_performance_regression(current_file, baseline_file, threshold_percent):
    """Check for performance regressions"""

    try:
        with open(current_file, 'r') as f:
            current_data = json.load(f)

        with open(baseline_file, 'r') as f:
            baseline_data = json.load(f)
    except FileNotFoundError as e:
        print(f"Warning: {e}")
        return True  # Pass if baseline doesn't exist yet

    regressions = []

    for benchmark in current_data.get('benchmarks', []):
        name = benchmark['name']
        current_time = benchmark['stats']['mean']

        # Find corresponding baseline
        baseline_benchmark = None
        for b in baseline_data.get('benchmarks', []):
            if b['name'] == name:
                baseline_benchmark = b
                break

        if not baseline_benchmark:
            print(f"New benchmark: {name}")
            continue

        baseline_time = baseline_benchmark['stats']['mean']

        # Calculate regression percentage
        if baseline_time > 0:
            regression_pct = ((current_time - baseline_time) / baseline_time) * 100

            if regression_pct > threshold_percent:
                regressions.append({
                    'name': name,
                    'regression_pct': regression_pct,
                    'baseline_time': baseline_time,
                    'current_time': current_time
                })

    if regressions:
        print("Performance regressions detected:")
        for regression in regressions:
            print(f"  {regression['name']}: {regression['regression_pct']:.1f}% slower")
            print(f"    Baseline: {regression['baseline_time']:.3f}s")
            print(f"    Current:  {regression['current_time']:.3f}s")
        return False
    else:
        print("No significant performance regressions detected")
        return True

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument('--benchmark-file', required=True)
    parser.add_argument('--baseline-file', required=True)
    parser.add_argument('--threshold', type=float, default=20.0)

    args = parser.parse_args()

    if not check_performance_regression(
        args.benchmark_file,
        args.baseline_file,
        args.threshold
    ):
        sys.exit(1)
```

## Running Performance Tests

### Command Line Examples

```bash
# Run response time tests
pytest tests/performance/test_response_times.py -v

# Run load tests with Locust
locust -f tests/performance/locustfile.py --host=http://localhost:8000

# Run memory profiling tests
python -m memory_profiler tests/performance/test_resource_usage.py

# Run with benchmark plugin
pytest tests/performance/ --benchmark-only --benchmark-json=results.json

# Generate performance report
pytest tests/performance/ --benchmark-only --benchmark-html=report.html

# Run N+1 detection tests
pytest tests/performance/test_n_plus_one.py -v --log-cli-level=DEBUG
```

### Performance Test Environment

```bash
# .env.performance
# Use production-like settings
DATABASE_URL=postgresql://perf:perf@localhost:5432/perf_db
FRAISEQL_LOG_LEVEL=WARNING  # Reduce logging overhead
FRAISEQL_CONNECTION_POOL_SIZE=20
FRAISEQL_CONNECTION_POOL_MAX_SIZE=50

# Performance test specific settings
PERFORMANCE_TEST_DURATION=300  # 5 minutes
PERFORMANCE_MAX_USERS=100
PERFORMANCE_SPAWN_RATE=10
```

Performance testing is critical for ensuring your FraiseQL application can handle production loads. Regular performance testing helps catch regressions early and ensures consistent user experience as your application grows.
