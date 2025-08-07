# Integration Testing

Integration tests verify that components work correctly together, especially database interactions and multi-component workflows. Unlike unit tests, integration tests use real databases and test the complete data flow.

## Database Test Setup

### Container-Based Testing

```python
# conftest.py
import pytest
import asyncio
import asyncpg
from testcontainers.postgres import PostgresContainer
from fraiseql.repository import FraiseQLRepository

@pytest.fixture(scope="session")
def event_loop():
    """Create event loop for async tests"""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()

@pytest.fixture(scope="session")
async def postgres_container():
    """PostgreSQL container for integration tests"""
    with PostgresContainer("postgres:15-alpine") as postgres:
        # Wait for container to be ready
        postgres.get_connection_url()
        yield postgres

@pytest.fixture(scope="session")
async def database_url(postgres_container):
    """Get database URL from container"""
    return postgres_container.get_connection_url()

@pytest.fixture
async def test_db(database_url):
    """Create test database with schema"""
    async with FraiseQLRepository(database_url) as repo:
        # Create test schema
        await repo.execute("""
            -- Create extensions
            CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

            -- User table
            CREATE TABLE IF NOT EXISTS tb_user (
                id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                name TEXT NOT NULL,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT NOW(),
                updated_at TIMESTAMP DEFAULT NOW()
            );

            -- Posts table
            CREATE TABLE IF NOT EXISTS tb_post (
                id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                author_id UUID NOT NULL REFERENCES tb_user(id),
                status TEXT DEFAULT 'draft',
                created_at TIMESTAMP DEFAULT NOW(),
                updated_at TIMESTAMP DEFAULT NOW()
            );

            -- Comments table
            CREATE TABLE IF NOT EXISTS tb_comment (
                id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                post_id UUID NOT NULL REFERENCES tb_post(id) ON DELETE CASCADE,
                author_id UUID NOT NULL REFERENCES tb_user(id),
                content TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT NOW()
            );

            -- Views for GraphQL
            CREATE OR REPLACE VIEW v_user AS
            SELECT id, jsonb_build_object(
                'id', id,
                'name', name,
                'email', email,
                'created_at', created_at,
                'updated_at', updated_at
            ) AS data
            FROM tb_user;

            CREATE OR REPLACE VIEW v_post AS
            SELECT id, jsonb_build_object(
                'id', id,
                'title', title,
                'content', content,
                'author_id', author_id,
                'status', status,
                'created_at', created_at,
                'updated_at', updated_at
            ) AS data
            FROM tb_post;

            CREATE OR REPLACE VIEW v_comment AS
            SELECT id, jsonb_build_object(
                'id', id,
                'post_id', post_id,
                'author_id', author_id,
                'content', content,
                'created_at', created_at
            ) AS data
            FROM tb_comment;

            -- Database functions
            CREATE OR REPLACE FUNCTION fn_create_user(
                p_name TEXT,
                p_email TEXT,
                p_password_hash TEXT
            ) RETURNS UUID AS $$
            DECLARE
                new_id UUID;
            BEGIN
                INSERT INTO tb_user (name, email, password_hash)
                VALUES (p_name, p_email, p_password_hash)
                RETURNING id INTO new_id;

                RETURN new_id;
            END;
            $$ LANGUAGE plpgsql;

            CREATE OR REPLACE FUNCTION fn_create_post(
                p_title TEXT,
                p_content TEXT,
                p_author_id UUID
            ) RETURNS UUID AS $$
            DECLARE
                new_id UUID;
            BEGIN
                INSERT INTO tb_post (title, content, author_id)
                VALUES (p_title, p_content, p_author_id)
                RETURNING id INTO new_id;

                RETURN new_id;
            END;
            $$ LANGUAGE plpgsql;

            CREATE OR REPLACE FUNCTION fn_delete_user(
                p_user_id UUID
            ) RETURNS BOOLEAN AS $$
            BEGIN
                DELETE FROM tb_user WHERE id = p_user_id;
                RETURN FOUND;
            END;
            $$ LANGUAGE plpgsql;
        """)

        yield repo

@pytest.fixture
async def sample_user(test_db):
    """Create a sample user for testing"""
    user_id = await test_db.call_function(
        "fn_create_user",
        p_name="Test User",
        p_email="test@example.com",
        p_password_hash="hashed_password"
    )

    user = await test_db.find_one("v_user", where={"id": user_id})
    return user

@pytest.fixture
async def sample_users(test_db):
    """Create multiple sample users for testing"""
    users = []
    for i in range(3):
        user_id = await test_db.call_function(
            "fn_create_user",
            p_name=f"User {i+1}",
            p_email=f"user{i+1}@example.com",
            p_password_hash="hashed_password"
        )
        user = await test_db.find_one("v_user", where={"id": user_id})
        users.append(user)

    return users
```

## Repository Integration Tests

### Basic CRUD Operations

```python
# test_repository_integration.py
import pytest
from fraiseql.repository import FraiseQLRepository
import asyncpg

@pytest.mark.asyncio
class TestRepositoryIntegration:
    async def test_create_and_fetch_user(self, test_db):
        """Test creating and fetching a user"""
        # Create user via database function
        user_id = await test_db.call_function(
            "fn_create_user",
            p_name="Integration Test User",
            p_email="integration@test.com",
            p_password_hash="secure_hash"
        )

        # Verify user_id was returned
        assert user_id is not None

        # Fetch created user
        user = await test_db.find_one("v_user", where={"id": user_id})

        # Verify user data
        assert user is not None
        assert user["name"] == "Integration Test User"
        assert user["email"] == "integration@test.com"
        assert "created_at" in user
        assert "updated_at" in user

    async def test_find_users_with_filters(self, test_db, sample_users):
        """Test finding users with various filters"""
        # Find all users
        all_users = await test_db.find("v_user")
        assert len(all_users) >= 3

        # Find user by email
        user = await test_db.find_one("v_user", where={"email": "user1@example.com"})
        assert user is not None
        assert user["name"] == "User 1"

        # Find users with name pattern
        users = await test_db.find("v_user", where={"name": {"like": "%User%"}})
        assert len(users) >= 3

        # Find with limit
        limited_users = await test_db.find("v_user", limit=2)
        assert len(limited_users) == 2

    async def test_update_user_data(self, test_db, sample_user):
        """Test updating user data"""
        user_id = sample_user["id"]

        # Update user directly via SQL
        await test_db.execute(
            "UPDATE tb_user SET name = $1, updated_at = NOW() WHERE id = $2",
            "Updated Name",
            user_id
        )

        # Fetch updated user
        updated_user = await test_db.find_one("v_user", where={"id": user_id})

        assert updated_user["name"] == "Updated Name"
        assert updated_user["updated_at"] != updated_user["created_at"]

    async def test_delete_user(self, test_db, sample_user):
        """Test deleting a user"""
        user_id = sample_user["id"]

        # Delete user via function
        deleted = await test_db.call_function("fn_delete_user", p_user_id=user_id)
        assert deleted is True

        # Verify user is gone
        user = await test_db.find_one("v_user", where={"id": user_id})
        assert user is None

    async def test_complex_query_with_joins(self, test_db, sample_user):
        """Test complex query with joins"""
        user_id = sample_user["id"]

        # Create some posts for the user
        for i in range(3):
            await test_db.call_function(
                "fn_create_post",
                p_title=f"Post {i+1}",
                p_content=f"Content for post {i+1}",
                p_author_id=user_id
            )

        # Query user with their posts count
        result = await test_db.execute("""
            SELECT
                u.name,
                u.email,
                COUNT(p.id) as post_count
            FROM tb_user u
            LEFT JOIN tb_post p ON u.id = p.author_id
            WHERE u.id = $1
            GROUP BY u.id, u.name, u.email
        """, user_id)

        row = result[0]
        assert row["name"] == sample_user["name"]
        assert row["post_count"] == 3
```

### Transaction Testing

```python
# test_transactions.py
import pytest
import asyncpg

@pytest.mark.asyncio
class TestTransactionHandling:
    async def test_transaction_commit(self, test_db):
        """Test successful transaction commit"""
        async with test_db.transaction() as tx:
            # Create user in transaction
            user_id = await tx.call_function(
                "fn_create_user",
                p_name="Transaction User",
                p_email="transaction@test.com",
                p_password_hash="hash"
            )

            # Create post for user in same transaction
            post_id = await tx.call_function(
                "fn_create_post",
                p_title="Transaction Post",
                p_content="Post content",
                p_author_id=user_id
            )

            # Transaction commits automatically on success

        # Verify both records were committed
        user = await test_db.find_one("v_user", where={"id": user_id})
        post = await test_db.find_one("v_post", where={"id": post_id})

        assert user is not None
        assert post is not None
        assert post["author_id"] == user_id

    async def test_transaction_rollback_on_exception(self, test_db):
        """Test automatic transaction rollback on exception"""
        # Try to create user with duplicate email
        await test_db.call_function(
            "fn_create_user",
            p_name="Existing User",
            p_email="existing@test.com",
            p_password_hash="hash"
        )

        with pytest.raises(asyncpg.UniqueViolationError):
            async with test_db.transaction() as tx:
                # This should work
                user_id = await tx.call_function(
                    "fn_create_user",
                    p_name="Valid User",
                    p_email="valid@test.com",
                    p_password_hash="hash"
                )

                # This should fail and rollback entire transaction
                await tx.call_function(
                    "fn_create_user",
                    p_name="Duplicate User",
                    p_email="existing@test.com",  # Duplicate email
                    p_password_hash="hash"
                )

        # Verify first user was rolled back
        user = await test_db.find_one("v_user", where={"email": "valid@test.com"})
        assert user is None

    async def test_manual_transaction_rollback(self, test_db):
        """Test manual transaction rollback"""
        try:
            async with test_db.transaction() as tx:
                # Create user
                user_id = await tx.call_function(
                    "fn_create_user",
                    p_name="Rollback User",
                    p_email="rollback@test.com",
                    p_password_hash="hash"
                )

                # Manually raise exception to trigger rollback
                raise Exception("Manual rollback")
        except Exception:
            pass  # Expected exception

        # Verify user was rolled back
        user = await test_db.find_one("v_user", where={"email": "rollback@test.com"})
        assert user is None

    async def test_nested_transactions(self, test_db):
        """Test nested transaction behavior (savepoints)"""
        async with test_db.transaction() as outer_tx:
            # Create user in outer transaction
            user_id = await outer_tx.call_function(
                "fn_create_user",
                p_name="Outer User",
                p_email="outer@test.com",
                p_password_hash="hash"
            )

            try:
                async with outer_tx.transaction() as inner_tx:
                    # Create post in inner transaction
                    await inner_tx.call_function(
                        "fn_create_post",
                        p_title="Inner Post",
                        p_content="Content",
                        p_author_id=user_id
                    )

                    # Force inner transaction to fail
                    raise Exception("Inner transaction fails")
            except Exception:
                pass  # Inner transaction rolled back

            # Outer transaction should still be valid
            # Create another post to verify
            post_id = await outer_tx.call_function(
                "fn_create_post",
                p_title="Outer Post",
                p_content="Content after inner rollback",
                p_author_id=user_id
            )

        # Verify outer transaction committed
        user = await test_db.find_one("v_user", where={"id": user_id})
        post = await test_db.find_one("v_post", where={"id": post_id})
        inner_post = await test_db.find_one("v_post", where={"title": "Inner Post"})

        assert user is not None
        assert post is not None  # Outer post committed
        assert inner_post is None  # Inner post rolled back
```

### Connection Pool Testing

```python
# test_connection_pool.py
import pytest
import asyncio
from fraiseql.repository import FraiseQLRepository

@pytest.mark.asyncio
class TestConnectionPooling:
    async def test_concurrent_database_operations(self, database_url):
        """Test concurrent database operations with connection pooling"""
        async def create_user(repo, index):
            """Helper to create a user"""
            return await repo.call_function(
                "fn_create_user",
                p_name=f"Concurrent User {index}",
                p_email=f"concurrent{index}@test.com",
                p_password_hash="hash"
            )

        # Create repository with connection pool
        async with FraiseQLRepository(
            database_url,
            min_size=5,
            max_size=20
        ) as repo:
            # Create test schema
            await repo.execute("""
                CREATE TABLE IF NOT EXISTS tb_user (
                    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                    name TEXT NOT NULL,
                    email TEXT UNIQUE NOT NULL,
                    password_hash TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT NOW()
                );
            """)

            # Create database function
            await repo.execute("""
                CREATE OR REPLACE FUNCTION fn_create_user(
                    p_name TEXT,
                    p_email TEXT,
                    p_password_hash TEXT
                ) RETURNS UUID AS $$
                DECLARE
                    new_id UUID;
                BEGIN
                    INSERT INTO tb_user (name, email, password_hash)
                    VALUES (p_name, p_email, p_password_hash)
                    RETURNING id INTO new_id;
                    RETURN new_id;
                END;
                $$ LANGUAGE plpgsql;
            """)

            # Run concurrent operations
            tasks = [
                create_user(repo, i)
                for i in range(50)  # More tasks than pool size
            ]

            user_ids = await asyncio.gather(*tasks)

            # Verify all users were created
            assert len(user_ids) == 50
            assert all(user_id is not None for user_id in user_ids)
            assert len(set(user_ids)) == 50  # All unique

    async def test_connection_pool_exhaustion_handling(self, database_url):
        """Test behavior when connection pool is exhausted"""
        # Create small connection pool
        async with FraiseQLRepository(
            database_url,
            min_size=2,
            max_size=3
        ) as repo:
            # Setup schema
            await repo.execute("""
                CREATE TABLE IF NOT EXISTS tb_test (
                    id SERIAL PRIMARY KEY,
                    value TEXT
                );
            """)

            async def long_running_query(index):
                """Query that holds connection for a while"""
                await repo.execute(
                    "INSERT INTO tb_test (value) VALUES ($1)",
                    f"value_{index}"
                )
                await asyncio.sleep(0.1)  # Hold connection briefly
                return index

            # Start more tasks than pool size
            tasks = [
                long_running_query(i)
                for i in range(10)  # More than pool size (3)
            ]

            # Should complete successfully, queuing requests
            start_time = asyncio.get_event_loop().time()
            results = await asyncio.gather(*tasks)
            end_time = asyncio.get_event_loop().time()

            assert len(results) == 10
            assert end_time - start_time > 0.1  # Some queuing occurred

    async def test_connection_recovery_after_error(self, test_db):
        """Test connection pool recovers from database errors"""
        # Cause a database error
        try:
            await test_db.execute("SELECT * FROM nonexistent_table")
        except Exception:
            pass  # Expected error

        # Verify pool still works after error
        user_id = await test_db.call_function(
            "fn_create_user",
            p_name="Recovery Test User",
            p_email="recovery@test.com",
            p_password_hash="hash"
        )

        user = await test_db.find_one("v_user", where={"id": user_id})
        assert user is not None
```

## Error Handling Integration Tests

```python
# test_error_handling.py
import pytest
import asyncpg
from fraiseql.exceptions import FraiseQLError

@pytest.mark.asyncio
class TestDatabaseErrorHandling:
    async def test_unique_constraint_violation(self, test_db):
        """Test handling of unique constraint violations"""
        # Create first user
        await test_db.call_function(
            "fn_create_user",
            p_name="First User",
            p_email="duplicate@test.com",
            p_password_hash="hash"
        )

        # Try to create user with same email
        with pytest.raises(asyncpg.UniqueViolationError):
            await test_db.call_function(
                "fn_create_user",
                p_name="Second User",
                p_email="duplicate@test.com",  # Same email
                p_password_hash="hash"
            )

    async def test_foreign_key_constraint_violation(self, test_db):
        """Test handling of foreign key constraint violations"""
        # Try to create post with non-existent author
        fake_user_id = "123e4567-e89b-12d3-a456-426614174000"

        with pytest.raises(asyncpg.ForeignKeyViolationError):
            await test_db.call_function(
                "fn_create_post",
                p_title="Orphaned Post",
                p_content="Content",
                p_author_id=fake_user_id  # Non-existent user
            )

    async def test_not_null_constraint_violation(self, test_db):
        """Test handling of NOT NULL constraint violations"""
        with pytest.raises(asyncpg.NotNullViolationError):
            await test_db.execute(
                "INSERT INTO tb_user (email, password_hash) VALUES ($1, $2)",
                "test@example.com",
                "hash"
                # Missing required 'name' field
            )

    async def test_connection_timeout_handling(self, database_url):
        """Test handling of connection timeouts"""
        # Create repository with very short timeout
        async with FraiseQLRepository(
            database_url,
            command_timeout=0.001  # 1ms timeout
        ) as repo:
            # Setup basic schema
            await repo.execute("CREATE TABLE IF NOT EXISTS tb_quick (id SERIAL PRIMARY KEY)")

            # This should timeout due to very short timeout
            with pytest.raises((asyncpg.CancelledError, asyncio.TimeoutError)):
                await repo.execute("SELECT pg_sleep(1)")  # Sleep for 1 second

    async def test_invalid_sql_handling(self, test_db):
        """Test handling of invalid SQL syntax"""
        with pytest.raises(asyncpg.PostgresSyntaxError):
            await test_db.execute("INVALID SQL SYNTAX HERE")

    async def test_permission_denied_handling(self, database_url):
        """Test handling of permission errors"""
        # Create connection with restricted user (if available in test env)
        # This test might need to be skipped in some environments
        pytest.skip("Requires restricted database user setup")
```

## Database Function Testing

```python
# test_database_functions.py
import pytest
from datetime import datetime

@pytest.mark.asyncio
class TestDatabaseFunctions:
    async def test_user_creation_function(self, test_db):
        """Test fn_create_user function behavior"""
        user_id = await test_db.call_function(
            "fn_create_user",
            p_name="Function Test User",
            p_email="function@test.com",
            p_password_hash="secure_hash_123"
        )

        # Verify function returned UUID
        assert user_id is not None
        assert len(str(user_id)) == 36  # UUID length

        # Verify user was created correctly
        user = await test_db.find_one("v_user", where={"id": user_id})
        assert user["name"] == "Function Test User"
        assert user["email"] == "function@test.com"
        assert "created_at" in user

    async def test_post_creation_function(self, test_db, sample_user):
        """Test fn_create_post function behavior"""
        author_id = sample_user["id"]

        post_id = await test_db.call_function(
            "fn_create_post",
            p_title="Test Post Title",
            p_content="This is test post content",
            p_author_id=author_id
        )

        assert post_id is not None

        # Verify post was created
        post = await test_db.find_one("v_post", where={"id": post_id})
        assert post["title"] == "Test Post Title"
        assert post["content"] == "This is test post content"
        assert post["author_id"] == author_id
        assert post["status"] == "draft"  # Default status

    async def test_delete_function_with_cascade(self, test_db, sample_user):
        """Test delete function with cascading effects"""
        user_id = sample_user["id"]

        # Create posts for the user
        post_ids = []
        for i in range(3):
            post_id = await test_db.call_function(
                "fn_create_post",
                p_title=f"Post {i}",
                p_content=f"Content {i}",
                p_author_id=user_id
            )
            post_ids.append(post_id)

        # Delete user (should cascade to posts)
        deleted = await test_db.call_function("fn_delete_user", p_user_id=user_id)
        assert deleted is True

        # Verify user and posts are gone
        user = await test_db.find_one("v_user", where={"id": user_id})
        assert user is None

        for post_id in post_ids:
            post = await test_db.find_one("v_post", where={"id": post_id})
            assert post is None
```

## Data Consistency Testing

```python
# test_data_consistency.py
import pytest
import asyncio

@pytest.mark.asyncio
class TestDataConsistency:
    async def test_concurrent_user_creation_uniqueness(self, test_db):
        """Test that concurrent user creation maintains email uniqueness"""
        async def create_user_with_email(email, index):
            try:
                return await test_db.call_function(
                    "fn_create_user",
                    p_name=f"User {index}",
                    p_email=email,
                    p_password_hash="hash"
                )
            except Exception as e:
                return f"error_{index}: {str(e)}"

        # Try to create multiple users with same email concurrently
        same_email = "concurrent@test.com"
        tasks = [
            create_user_with_email(same_email, i)
            for i in range(10)
        ]

        results = await asyncio.gather(*tasks, return_exceptions=True)

        # Count successful creations
        successful_results = [
            r for r in results
            if isinstance(r, str) and len(r) == 36  # UUID length
        ]

        # Only one should succeed due to unique constraint
        assert len(successful_results) == 1

    async def test_referential_integrity_under_load(self, test_db, sample_users):
        """Test referential integrity with concurrent operations"""
        user_ids = [user["id"] for user in sample_users]

        async def create_post_for_random_user(index):
            import random
            user_id = random.choice(user_ids)
            try:
                return await test_db.call_function(
                    "fn_create_post",
                    p_title=f"Concurrent Post {index}",
                    p_content=f"Content {index}",
                    p_author_id=user_id
                )
            except Exception as e:
                return f"error: {str(e)}"

        # Create many posts concurrently
        tasks = [create_post_for_random_user(i) for i in range(50)]
        results = await asyncio.gather(*tasks)

        # Count successful post creations
        successful_posts = [
            r for r in results
            if isinstance(r, str) and len(r) == 36  # UUID length
        ]

        assert len(successful_posts) == 50  # All should succeed

        # Verify all posts have valid authors
        for post_id in successful_posts:
            post = await test_db.find_one("v_post", where={"id": post_id})
            assert post is not None
            assert post["author_id"] in user_ids
```

## Performance Integration Tests

```python
# test_performance_integration.py
import pytest
import time
import asyncio

@pytest.mark.asyncio
class TestPerformanceIntegration:
    async def test_bulk_insert_performance(self, test_db):
        """Test performance of bulk insert operations"""
        start_time = time.perf_counter()

        # Create 1000 users
        user_creation_tasks = []
        for i in range(1000):
            task = test_db.call_function(
                "fn_create_user",
                p_name=f"Bulk User {i}",
                p_email=f"bulk{i}@test.com",
                p_password_hash="hash"
            )
            user_creation_tasks.append(task)

        user_ids = await asyncio.gather(*user_creation_tasks)

        end_time = time.perf_counter()
        elapsed = end_time - start_time

        # Assert performance requirements
        assert len(user_ids) == 1000
        assert elapsed < 10.0  # Should complete within 10 seconds

        # Calculate rate
        rate = 1000 / elapsed
        print(f"User creation rate: {rate:.2f} users/second")

        # Verify data integrity
        actual_count = await test_db.fetchval("SELECT COUNT(*) FROM tb_user")
        assert actual_count >= 1000

    async def test_query_performance_with_large_dataset(self, test_db):
        """Test query performance with larger dataset"""
        # Create test data (skip if already exists from previous test)
        existing_count = await test_db.fetchval("SELECT COUNT(*) FROM tb_user")
        if existing_count < 1000:
            pytest.skip("Requires bulk data from previous test")

        # Test query performance
        start_time = time.perf_counter()

        # Find users with email pattern
        users = await test_db.find(
            "v_user",
            where={"email": {"like": "%bulk%"}},
            limit=100
        )

        end_time = time.perf_counter()
        elapsed = end_time - start_time

        assert len(users) == 100
        assert elapsed < 1.0  # Should complete within 1 second

        print(f"Query time for 100 results: {elapsed*1000:.2f}ms")

    async def test_connection_pool_efficiency(self, database_url):
        """Test connection pool efficiency under load"""
        async with FraiseQLRepository(
            database_url,
            min_size=10,
            max_size=20
        ) as repo:
            # Setup test table
            await repo.execute("""
                CREATE TABLE IF NOT EXISTS tb_perf_test (
                    id SERIAL PRIMARY KEY,
                    value TEXT,
                    created_at TIMESTAMP DEFAULT NOW()
                )
            """)

            async def quick_operation(index):
                await repo.execute(
                    "INSERT INTO tb_perf_test (value) VALUES ($1)",
                    f"value_{index}"
                )
                return await repo.fetchval(
                    "SELECT COUNT(*) FROM tb_perf_test WHERE value = $1",
                    f"value_{index}"
                )

            # Measure time for 200 concurrent operations
            start_time = time.perf_counter()

            tasks = [quick_operation(i) for i in range(200)]
            results = await asyncio.gather(*tasks)

            end_time = time.perf_counter()
            elapsed = end_time - start_time

            assert len(results) == 200
            assert all(r == 1 for r in results)  # Each insert/select succeeded

            # Should complete quickly with good connection pooling
            assert elapsed < 5.0  # Within 5 seconds
            print(f"200 operations completed in {elapsed:.2f} seconds")
```

## Running Integration Tests

### Command Line Examples

```bash
# Run all integration tests
pytest tests/integration/ -v

# Run with database container logs
pytest tests/integration/ -v -s

# Run specific test class
pytest tests/integration/test_repository_integration.py::TestRepositoryIntegration -v

# Run with coverage
pytest tests/integration/ --cov=app --cov-report=html

# Run only fast integration tests (skip performance tests)
pytest tests/integration/ -v -m "not slow"

# Run with parallel execution (be careful with database tests)
pytest tests/integration/ -n 2 --dist=loadgroup
```

### Environment Variables

```bash
# Set test database URL if not using containers
export TEST_DATABASE_URL=postgresql://test:test@localhost:5432/test_db

# Enable detailed database logging
export FRAISEQL_LOG_LEVEL=DEBUG

# Disable testcontainers cleanup for debugging
export TESTCONTAINERS_RYUK_DISABLED=true
```

Integration tests typically take longer than unit tests (5-30 seconds each) but provide high confidence that your application works correctly with real databases. They're essential for catching issues that mocks can't reveal, such as SQL syntax errors, constraint violations, and transaction behavior.
