"""Common DataLoader implementations."""

from collections import defaultdict
from typing import Any
from uuid import UUID

from fraiseql.optimization.dataloader import DataLoader


class UserLoader(DataLoader[UUID, dict[str, Any]]):
    """DataLoader for loading users by ID."""

    def __init__(self, db) -> None:
        super().__init__()
        self.db = db

    async def batch_load(self, user_ids: list[UUID]) -> list[dict | None]:
        """Load multiple users in one query."""
        # Single query for all users
        rows = await self.db.fetch_all(
            """
            SELECT * FROM users
            WHERE id = ANY($1::uuid[])
            """,
            user_ids,
        )

        # Convert to dict for sorting
        users = [dict(row) for row in rows]

        # Return in same order as requested
        return self.sort_by_keys(users, user_ids)


class ProjectLoader(DataLoader[UUID, dict[str, Any]]):
    """DataLoader for loading projects by ID."""

    def __init__(self, db) -> None:
        super().__init__()
        self.db = db

    async def batch_load(self, project_ids: list[UUID]) -> list[dict | None]:
        """Load multiple projects in one query."""
        rows = await self.db.fetch_all(
            """
            SELECT * FROM projects
            WHERE id = ANY($1::uuid[])
            """,
            project_ids,
        )

        projects = [dict(row) for row in rows]
        return self.sort_by_keys(projects, project_ids)


class TasksByProjectLoader(DataLoader[UUID, list[dict[str, Any]]]):
    """DataLoader for loading tasks by project ID."""

    def __init__(self, db, limit: int = 100) -> None:
        super().__init__()
        self.db = db
        self.limit = limit

    async def batch_load(self, project_ids: list[UUID]) -> list[list[dict]]:
        """Load tasks for multiple projects."""
        # Use window function for efficient loading
        rows = await self.db.fetch_all(
            """
            WITH ranked_tasks AS (
                SELECT *,
                    ROW_NUMBER() OVER (
                        PARTITION BY project_id
                        ORDER BY created_at DESC
                    ) as rn
                FROM tasks
                WHERE project_id = ANY($1::uuid[])
            )
            SELECT * FROM ranked_tasks
            WHERE rn <= $2
            ORDER BY project_id, created_at DESC
            """,
            project_ids,
            self.limit,
        )

        # Group by project
        tasks_by_project = defaultdict(list)
        for row in rows:
            tasks_by_project[row["project_id"]].append(dict(row))

        # Return in order
        return [tasks_by_project.get(pid, []) for pid in project_ids]


class GenericForeignKeyLoader(DataLoader[UUID, dict[str, Any]]):
    """Generic loader for foreign key relationships."""

    def __init__(self, db, table: str, key_field: str = "id") -> None:
        super().__init__()
        self.db = db
        self.table = table
        self.key_field = key_field

    async def batch_load(self, keys: list[UUID]) -> list[dict | None]:
        """Load multiple records by key."""
        # CRITICAL: Enhanced SQL injection prevention
        if not self.table.replace("_", "").replace(".", "").isalnum():
            msg = f"Invalid table name: {self.table}"
            raise ValueError(msg)

        # CRITICAL: Validate key_field to prevent SQL injection
        if not self.key_field.replace("_", "").isalnum():
            msg = f"Invalid key field: {self.key_field}"
            raise ValueError(msg)

        # CRITICAL: Validate keys to prevent injection
        if not all(isinstance(k, str | int | bytes) or hasattr(k, "__str__") for k in keys):
            msg = "All keys must be safely serializable"
            raise ValueError(msg)

        # Use parameterized query construction
        query = f"""
            SELECT * FROM {self.table}
            WHERE {self.key_field} = ANY($1::uuid[])
        """

        rows = await self.db.fetch_all(query, keys)
        items = [dict(row) for row in rows]

        return self.sort_by_keys(items, keys, self.key_field)
