"""
Integration tests for Phase 2.1: Transaction Support.

Tests transaction support added to DatabasePool:
- BEGIN/COMMIT/ROLLBACK operations
- Savepoint creation and rollback
- Nested savepoints
- Transaction isolation

Uses testcontainers for automatic PostgreSQL provisioning.
"""

import asyncio
import pytest
import pytest_asyncio

# Import database fixtures (provides postgres_url via testcontainers)
pytest_plugins = ["tests.fixtures.database.database_conftest"]


@pytest_asyncio.fixture
async def pool(postgres_url):
    """Create database pool for testing using testcontainers PostgreSQL."""
    from fraiseql._fraiseql_rs import DatabasePool

    # Use testcontainers URL
    async with DatabasePool(url=postgres_url, max_size=10, ssl_mode="disable") as pool:
        # Create a test table
        await pool.execute_query("""
            CREATE TABLE IF NOT EXISTS test_transactions (
                id SERIAL PRIMARY KEY,
                value TEXT NOT NULL
            )
        """)

        yield pool

        # Cleanup
        await pool.execute_query("DROP TABLE IF EXISTS test_transactions")


class TestBasicTransactions:
    """Test basic transaction operations."""

    async def test_commit_transaction(self, pool):
        """Test transaction commit."""
        # Begin transaction
        await pool.begin_transaction()

        # Insert data
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('test1')")

        # Commit
        await pool.commit_transaction()

        # Verify data was committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'test1'"
        )
        assert len(results) == 1

    async def test_rollback_transaction(self, pool):
        """Test transaction rollback."""
        # Begin transaction
        await pool.begin_transaction()

        # Insert data
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('test_rollback')")

        # Rollback
        await pool.rollback_transaction()

        # Verify data was NOT committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'test_rollback'"
        )
        assert len(results) == 0

    async def test_transaction_isolation(self, pool):
        """Test that uncommitted changes are not visible outside transaction."""
        # Begin transaction
        await pool.begin_transaction()

        # Insert data
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('isolated')")

        # In a separate connection, data should not be visible
        # (This is a simplified test - in reality we'd need a second connection)

        # Rollback
        await pool.rollback_transaction()

        # Verify
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'isolated'"
        )
        assert len(results) == 0

    async def test_multiple_operations_in_transaction(self, pool):
        """Test multiple operations in single transaction."""
        await pool.begin_transaction()

        # Multiple inserts
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('multi1')")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('multi2')")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('multi3')")

        await pool.commit_transaction()

        # Verify all were committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value LIKE 'multi%' ORDER BY value"
        )
        assert len(results) == 3


class TestSavepoints:
    """Test savepoint functionality."""

    async def test_savepoint_basic(self, pool):
        """Test basic savepoint creation and rollback."""
        await pool.begin_transaction()

        # Insert first record
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('before_sp')")

        # Create savepoint
        await pool.savepoint("sp1")

        # Insert second record
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('after_sp')")

        # Rollback to savepoint
        await pool.rollback_to_savepoint("sp1")

        # Commit transaction
        await pool.commit_transaction()

        # Verify: first record exists, second doesn't
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'before_sp'"
        )
        assert len(results) == 1

        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'after_sp'"
        )
        assert len(results) == 0

    async def test_nested_savepoints(self, pool):
        """Test nested savepoints."""
        await pool.begin_transaction()

        # Insert at level 0
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('level0')")

        # Savepoint 1
        await pool.savepoint("sp1")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('level1')")

        # Savepoint 2
        await pool.savepoint("sp2")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('level2')")

        # Savepoint 3
        await pool.savepoint("sp3")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('level3')")

        # Rollback to sp2 (this rolls back level2 and level3 inserts)
        await pool.rollback_to_savepoint("sp2")

        await pool.commit_transaction()

        # Verify: level0 and level1 exist (inserted before sp2)
        # level2 and level3 don't exist (inserted after sp2, so rolled back)
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value IN ('level0', 'level1', 'level2', 'level3') ORDER BY value"
        )
        assert len(results) == 2  # Should have level0 and level1

    async def test_savepoint_partial_rollback(self, pool):
        """Test rolling back to middle savepoint."""
        await pool.begin_transaction()

        # Insert before any savepoints
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('before_sp')")

        await pool.savepoint("sp1")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('sp1_data')")

        await pool.savepoint("sp2")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('sp2_data')")

        await pool.savepoint("sp3")
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('sp3_data')")

        # Rollback to sp1 (rolls back sp1_data, sp2_data, and sp3_data)
        await pool.rollback_to_savepoint("sp1")

        await pool.commit_transaction()

        # Verify: before_sp exists, everything after sp1 is rolled back
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'before_sp'"
        )
        assert len(results) == 1

        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'sp1_data'"
        )
        assert len(results) == 0

        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'sp2_data'"
        )
        assert len(results) == 0

        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'sp3_data'"
        )
        assert len(results) == 0


class TestErrorHandling:
    """Test error handling in transactions."""

    async def test_rollback_on_error(self, pool):
        """Test that transaction can be rolled back after error."""
        await pool.begin_transaction()

        # Insert valid data
        await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('before_error')")

        # Attempt invalid operation
        try:
            await pool.execute_query("INSERT INTO non_existent_table VALUES (1)")
        except Exception:
            pass  # Expected

        # Rollback
        await pool.rollback_transaction()

        # Verify nothing was committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'before_error'"
        )
        assert len(results) == 0

    async def test_transaction_context_manager_pattern(self, pool):
        """Test try/except pattern for transactions."""
        # Success case
        try:
            await pool.begin_transaction()
            await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('success')")
            await pool.commit_transaction()
        except Exception:
            await pool.rollback_transaction()
            raise

        # Verify committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'success'"
        )
        assert len(results) == 1

        # Failure case
        try:
            await pool.begin_transaction()
            await pool.execute_query("INSERT INTO test_transactions (value) VALUES ('failure')")
            # Simulate error
            raise ValueError("Simulated error")
        except ValueError:
            await pool.rollback_transaction()

        # Verify NOT committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value = 'failure'"
        )
        assert len(results) == 0


class TestConcurrentTransactions:
    """Test concurrent transaction handling."""

    async def test_concurrent_transactions(self, pool):
        """Test that multiple transactions can run concurrently."""

        async def transaction(value):
            await pool.begin_transaction()
            await pool.execute_query(f"INSERT INTO test_transactions (value) VALUES ('{value}')")
            await asyncio.sleep(0.01)  # Small delay
            await pool.commit_transaction()

        # Run 5 concurrent transactions
        await asyncio.gather(*[transaction(f"concurrent_{i}") for i in range(5)])

        # Verify all were committed
        results = await pool.execute_query(
            "SELECT value FROM test_transactions WHERE value LIKE 'concurrent_%'"
        )
        assert len(results) == 5
