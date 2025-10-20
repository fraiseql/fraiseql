import pytest
from uuid import uuid4
from pathlib import Path
from fraiseql.db import DatabaseQuery


@pytest.fixture(autouse=True, scope="module")
async def setup_rbac_schema(db_pool):
    """Set up RBAC schema before running tests."""
    # Read the migration file
    migration_path = Path("src/fraiseql/enterprise/migrations/002_rbac_tables.sql")
    migration_sql = migration_path.read_text()

    # Execute the migration
    async with db_pool.connection() as conn:
        async with conn.cursor() as cur:
            await cur.execute(migration_sql)
            await conn.commit()


async def test_rbac_tables_exist(db_repo):
    """Verify RBAC tables exist with correct schema."""
    tables = ["roles", "permissions", "role_permissions", "user_roles"]

    for table in tables:
        result = await db_repo.run(
            DatabaseQuery(
                statement=f"""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = '{table}'
                ORDER BY column_name
            """,
                params={},
                fetch_result=True,
            )
        )
        assert len(result) > 0, f"Table {table} should exist"


async def test_roles_table_structure(db_repo):
    """Verify roles table has correct structure."""
    columns = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_name = 'roles'
            ORDER BY column_name
        """,
            params={},
            fetch_result=True,
        )
    )

    column_dict = {col["column_name"]: col for col in columns}

    # Required columns
    assert "id" in column_dict
    assert column_dict["id"]["data_type"] == "uuid"
    assert column_dict["id"]["is_nullable"] == "NO"

    assert "name" in column_dict
    assert column_dict["name"]["data_type"] == "character varying"
    assert column_dict["name"]["is_nullable"] == "NO"

    assert "description" in column_dict
    assert column_dict["description"]["data_type"] == "text"
    assert column_dict["description"]["is_nullable"] == "YES"

    assert "parent_role_id" in column_dict
    assert column_dict["parent_role_id"]["data_type"] == "uuid"
    assert column_dict["parent_role_id"]["is_nullable"] == "YES"

    assert "tenant_id" in column_dict
    assert column_dict["tenant_id"]["data_type"] == "uuid"
    assert column_dict["tenant_id"]["is_nullable"] == "YES"

    assert "is_system" in column_dict
    assert column_dict["is_system"]["data_type"] == "boolean"
    assert column_dict["is_system"]["is_nullable"] == "YES"

    assert "created_at" in column_dict
    assert "updated_at" in column_dict


async def test_permissions_table_structure(db_repo):
    """Verify permissions table has correct structure."""
    columns = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_name = 'permissions'
            ORDER BY column_name
        """,
            params={},
            fetch_result=True,
        )
    )

    column_dict = {col["column_name"]: col for col in columns}

    assert "id" in column_dict
    assert "resource" in column_dict
    assert "action" in column_dict
    assert "description" in column_dict
    assert "constraints" in column_dict
    assert "created_at" in column_dict


async def test_role_permissions_table_structure(db_repo):
    """Verify role_permissions table has correct structure."""
    columns = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_name = 'role_permissions'
            ORDER BY column_name
        """,
            params={},
            fetch_result=True,
        )
    )

    column_dict = {col["column_name"]: col for col in columns}

    assert "id" in column_dict
    assert "role_id" in column_dict
    assert "permission_id" in column_dict
    assert "granted" in column_dict
    assert "created_at" in column_dict


async def test_user_roles_table_structure(db_repo):
    """Verify user_roles table has correct structure."""
    columns = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_name = 'user_roles'
            ORDER BY column_name
        """,
            params={},
            fetch_result=True,
        )
    )

    column_dict = {col["column_name"]: col for col in columns}

    assert "id" in column_dict
    assert "user_id" in column_dict
    assert "role_id" in column_dict
    assert "tenant_id" in column_dict
    assert "granted_by" in column_dict
    assert "granted_at" in column_dict
    assert "expires_at" in column_dict


async def test_get_inherited_roles_function_exists(db_repo):
    """Verify get_inherited_roles function exists."""
    result = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT routine_name
            FROM information_schema.routines
            WHERE routine_name = 'get_inherited_roles'
            AND routine_type = 'FUNCTION'
        """,
            params={},
            fetch_result=True,
        )
    )

    assert len(result) == 1, "get_inherited_roles function should exist"


async def test_seed_data_exists(db_repo):
    """Verify seed data was inserted."""
    # Check system roles
    roles = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT name, is_system, parent_role_id
            FROM roles
            WHERE is_system = TRUE
            ORDER BY name
        """,
            params={},
            fetch_result=True,
        )
    )

    role_names = [r["name"] for r in roles]
    assert "admin" in role_names
    assert "manager" in role_names
    assert "super_admin" in role_names
    assert "user" in role_names
    assert "viewer" in role_names

    # Check permissions
    permissions = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT resource, action
            FROM permissions
            ORDER BY resource, action
        """,
            params={},
            fetch_result=True,
        )
    )

    assert len(permissions) >= 8  # At least the seed permissions

    permission_strings = [f"{p['resource']}.{p['action']}" for p in permissions]
    assert "user.create" in permission_strings
    assert "user.read" in permission_strings
    assert "user.update" in permission_strings
    assert "user.delete" in permission_strings
    assert "role.assign" in permission_strings


async def test_role_hierarchy_function_works(db_repo):
    """Test that the role hierarchy function works with seed data."""
    # Get the viewer role ID
    viewer_role = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT id FROM roles WHERE name = 'viewer' AND is_system = TRUE
        """,
            params={},
            fetch_result=True,
        )
    )

    assert len(viewer_role) == 1
    viewer_id = viewer_role[0]["id"]

    # Get inherited roles
    inherited = await db_repo.run(
        DatabaseQuery(
            statement="""
            SELECT role_id, depth FROM get_inherited_roles(%(role_id)s)
            ORDER BY depth
        """,
            params={"role_id": str(viewer_id)},
            fetch_result=True,
        )
    )

    # Should inherit from viewer -> user -> manager -> admin -> super_admin
    assert len(inherited) == 5  # viewer + 4 parents

    # Check depths
    depths = [row["depth"] for row in inherited]
    assert 0 in depths  # self
    assert 1 in depths  # immediate parent
    assert 2 in depths  # grandparent
    assert 3 in depths  # great-grandparent
    assert 4 in depths  # great-great-grandparent
