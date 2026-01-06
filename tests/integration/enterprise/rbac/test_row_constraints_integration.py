"""Integration tests for row-level constraint system.

Tests the complete row-level authorization flow from middleware through database.
"""

from pathlib import Path
from uuid import uuid4

import pytest
import pytest_asyncio

from fraiseql.db import DatabaseQuery

pytestmark = pytest.mark.integration


@pytest_asyncio.fixture(autouse=True, scope="class")
async def setup_row_constraints_schema(class_db_pool, test_schema) -> None:
    """Set up row constraints schema before running tests."""
    # Read and execute RBAC migration first
    rbac_migration_path = Path("src/fraiseql/enterprise/migrations/002_rbac_tables.sql")
    rbac_migration_sql = rbac_migration_path.read_text()

    # Read and execute row constraints migration
    row_constraints_path = Path("src/fraiseql/enterprise/migrations/005_row_constraint_tables.sql")
    row_constraints_sql = row_constraints_path.read_text()

    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")

        # Execute RBAC schema
        await conn.execute(rbac_migration_sql)

        # Execute row constraints schema
        await conn.execute(row_constraints_sql)

        await conn.commit()
        print("Row constraints schema migration executed successfully")


class TestRowConstraintTableStructure:
    """Test row constraint table schema."""

    @pytest.mark.asyncio
    async def test_tb_row_constraint_table_exists(self, db_repo) -> None:
        """Verify tb_row_constraint table exists."""
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = 'tb_row_constraint'
                ORDER BY column_name
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, "tb_row_constraint table should exist"

        # Check key columns
        column_names = [row[0] for row in result]
        assert "id" in column_names
        assert "table_name" in column_names
        assert "role_id" in column_names
        assert "constraint_type" in column_names
        assert "field_name" in column_names

    @pytest.mark.asyncio
    async def test_tb_row_constraint_audit_table_exists(self, db_repo) -> None:
        """Verify tb_row_constraint_audit table exists."""
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT column_name
                FROM information_schema.columns
                WHERE table_name = 'tb_row_constraint_audit'
                ORDER BY column_name
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, "tb_row_constraint_audit table should exist"

        column_names = [row[0] for row in result]
        assert "constraint_id" in column_names
        assert "user_id" in column_names
        assert "action" in column_names


class TestRowConstraintIndexes:
    """Test row constraint indexes."""

    @pytest.mark.asyncio
    async def test_primary_index_exists(self, db_repo) -> None:
        """Verify primary index on (table_name, role_id)."""
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT indexname
                FROM pg_indexes
                WHERE tablename = 'tb_row_constraint'
                AND indexname LIKE '%table_role%'
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, "Primary index should exist"

    @pytest.mark.asyncio
    async def test_audit_indexes_exist(self, db_repo) -> None:
        """Verify audit table indexes."""
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT COUNT(*) as index_count
                FROM pg_indexes
                WHERE tablename = 'tb_row_constraint_audit'
            """,
                params={},
                fetch_result=True,
            )
        )
        count = result[0][0]
        assert count >= 2, "Should have at least 2 audit indexes"


class TestRowConstraintCreation:
    """Test creating row constraints."""

    @pytest.mark.asyncio
    async def test_create_ownership_constraint(self, db_repo) -> None:
        """Create and verify ownership constraint."""
        role_id = uuid4()
        table_name = "documents"

        # Insert constraint
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
            """,
                params=[table_name, role_id, "ownership", "owner_id"],
                fetch_result=False,
            )
        )

        # Verify insertion
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT constraint_type, field_name
                FROM tb_row_constraint
                WHERE table_name = $1 AND role_id = $2
            """,
                params=[table_name, role_id],
                fetch_result=True,
            )
        )

        assert len(result) == 1
        assert result[0][0] == "ownership"
        assert result[0][1] == "owner_id"

    @pytest.mark.asyncio
    async def test_create_tenant_constraint(self, db_repo) -> None:
        """Create and verify tenant constraint."""
        role_id = uuid4()
        table_name = "documents"

        # Insert constraint
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
            """,
                params=[table_name, role_id, "tenant", "tenant_id"],
                fetch_result=False,
            )
        )

        # Verify insertion
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT constraint_type, field_name
                FROM tb_row_constraint
                WHERE table_name = $1 AND role_id = $2
            """,
                params=[table_name, role_id],
                fetch_result=True,
            )
        )

        assert len(result) == 1
        assert result[0][0] == "tenant"
        assert result[0][1] == "tenant_id"

    @pytest.mark.asyncio
    async def test_unique_constraint_violation(self, db_repo) -> None:
        """Unique constraint prevents duplicate constraints."""
        role_id = uuid4()
        table_name = "documents"

        # Insert first constraint
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
            """,
                params=[table_name, role_id, "ownership", "owner_id"],
                fetch_result=False,
            )
        )

        # Try to insert duplicate - should fail
        with pytest.raises(Exception):  # Database constraint violation  # noqa: B017
            await db_repo.run(
                DatabaseQuery(
                    statement="""
                    INSERT INTO tb_row_constraint
                    (table_name, role_id, constraint_type, field_name)
                    VALUES ($1, $2, $3, $4)
                """,
                    params=[table_name, role_id, "ownership", "different_field"],
                    fetch_result=False,
                )
            )


class TestRowConstraintAudit:
    """Test audit trigger and audit table."""

    @pytest.mark.asyncio
    async def test_audit_trigger_on_insert(self, db_repo) -> None:
        """Audit trigger fires on INSERT."""
        # Set app context for audit
        await db_repo.run(
            DatabaseQuery(
                statement="SET app.user_id = $1",
                params=["11111111-1111-1111-1111-111111111111"],
                fetch_result=False,
            )
        )

        role_id = uuid4()
        table_name = "test_documents"

        # Insert constraint
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
            """,
                params=[table_name, role_id, "ownership", "owner_id"],
                fetch_result=False,
            )
        )

        # Verify audit entry was created
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT action, new_values
                FROM tb_row_constraint_audit
                WHERE action = 'CREATE'
                ORDER BY created_at DESC
                LIMIT 1
            """,
                params={},
                fetch_result=True,
            )
        )

        assert len(result) > 0, "Audit entry should be created"
        assert result[0][0] == "CREATE"

    @pytest.mark.asyncio
    async def test_audit_trigger_on_update(self, db_repo) -> None:
        """Audit trigger fires on UPDATE."""
        role_id = uuid4()
        table_name = "test_update"

        # Insert constraint
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
            """,
                params=[table_name, role_id, "ownership", "owner_id"],
                fetch_result=False,
            )
        )

        # Update constraint
        await db_repo.run(
            DatabaseQuery(
                statement="""
                UPDATE tb_row_constraint
                SET field_name = 'new_owner_field'
                WHERE table_name = $1 AND role_id = $2
            """,
                params=[table_name, role_id],
                fetch_result=False,
            )
        )

        # Verify audit entry
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT COUNT(*) as count
                FROM tb_row_constraint_audit
                WHERE action = 'UPDATE'
            """,
                params={},
                fetch_result=True,
            )
        )

        assert result[0][0] > 0, "UPDATE audit entry should exist"


class TestGetUserRowConstraintsFunctions:
    """Test PostgreSQL functions for constraint lookup."""

    @pytest.mark.asyncio
    async def test_get_user_row_constraints_function_exists(self, db_repo) -> None:
        """Function get_user_row_constraints should exist."""
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT proname
                FROM pg_proc
                WHERE proname = 'get_user_row_constraints'
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, "Function should exist"

    @pytest.mark.asyncio
    async def test_user_has_row_constraint_function_exists(self, db_repo) -> None:
        """Function user_has_row_constraint should exist."""
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT proname
                FROM pg_proc
                WHERE proname = 'user_has_row_constraint'
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, "Function should exist"


class TestConstraintCascadingDelete:
    """Test cascading deletes."""

    @pytest.mark.asyncio
    async def test_constraint_deleted_on_role_delete(self, db_repo) -> None:
        """Constraint deleted when role is deleted."""
        # Get an existing role
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT id FROM roles LIMIT 1
            """,
                params={},
                fetch_result=True,
            )
        )

        if not result:
            pytest.skip("No roles in database")

        role_id = result[0][0]

        # Insert constraint for this role
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT DO NOTHING
            """,
                params=["documents", role_id, "ownership", "owner_id"],
                fetch_result=False,
            )
        )

        # Verify constraint exists
        count_before = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT COUNT(*) FROM tb_row_constraint
                WHERE role_id = $1
            """,
                params=[role_id],
                fetch_result=True,
            )
        )
        assert count_before[0][0] > 0

        # Delete role
        await db_repo.run(
            DatabaseQuery(
                statement="""
                DELETE FROM roles WHERE id = $1
            """,
                params=[role_id],
                fetch_result=False,
            )
        )

        # Verify constraint was deleted
        count_after = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT COUNT(*) FROM tb_row_constraint
                WHERE role_id = $1
            """,
                params=[role_id],
                fetch_result=True,
            )
        )
        assert count_after[0][0] == 0


class TestMultiTenantIsolation:
    """Test multi-tenant constraint isolation."""

    @pytest.mark.asyncio
    async def test_constraints_per_tenant(self, db_repo) -> None:
        """Different tenants can have different constraints."""
        role1 = uuid4()
        role2 = uuid4()
        table_name = "documents"

        # Create roles
        for role_id in [role1, role2]:
            await db_repo.run(
                DatabaseQuery(
                    statement="""
                    INSERT INTO roles (id, name)
                    VALUES ($1, $2)
                    ON CONFLICT DO NOTHING
                """,
                    params=[role_id, f"role-{role_id}"],
                    fetch_result=False,
                )
            )

        # Create constraint for role1
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT DO NOTHING
            """,
                params=[table_name, role1, "ownership", "owner_id"],
                fetch_result=False,
            )
        )

        # Create constraint for role2
        await db_repo.run(
            DatabaseQuery(
                statement="""
                INSERT INTO tb_row_constraint
                (table_name, role_id, constraint_type, field_name)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT DO NOTHING
            """,
                params=[table_name, role2, "tenant", "tenant_id"],
                fetch_result=False,
            )
        )

        # Verify both exist and are different
        result = await db_repo.run(
            DatabaseQuery(
                statement="""
                SELECT constraint_type
                FROM tb_row_constraint
                WHERE table_name = $1
                ORDER BY role_id
            """,
                params=[table_name],
                fetch_result=True,
            )
        )

        assert len(result) >= 2
        constraint_types = [row[0] for row in result]
        assert "ownership" in constraint_types
        assert "tenant" in constraint_types
