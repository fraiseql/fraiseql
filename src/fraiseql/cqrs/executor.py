"""CQRS SQL function executor for FraiseQL."""

from typing import Any

from psycopg import AsyncConnection
from psycopg.rows import dict_row
from psycopg.sql import SQL


class CQRSExecutor:
    """Executes SQL functions for CQRS write operations."""

    def __init__(self, connection: AsyncConnection) -> None:
        """Initialize executor with database connection."""
        self.connection = connection

    async def execute_function(
        self,
        function_name: str,
        input_data: dict[str, Any],
    ) -> dict[str, Any]:
        """Execute a SQL function that returns mutation_result type.

        Args:
            function_name: Name of the SQL function to execute (e.g., 'graphql.create_user')
            input_data: Dictionary of input data to pass to the function

        Returns:
            Dictionary with the mutation result fields
        """
        async with self.connection.cursor(row_factory=dict_row) as cursor:
            # All mutation functions must return mutation_result composite type
            query = SQL(
                """
                SELECT
                    (result).id,
                    (result).updated_fields,
                    (result).status,
                    (result).message,
                    (result).object_data,
                    (result).extra_metadata
                FROM (
                    SELECT {}(%s::jsonb) as result
                ) t
            """,
            ).format(SQL(function_name))
            # Import here to avoid circular import
            from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder
            encoder = FraiseQLJSONEncoder()
            await cursor.execute(query, (encoder.encode(input_data),))
            result = await cursor.fetchone()

            if not result:
                return {
                    "status": "error",
                    "message": "No result returned from function",
                }

            return dict(result)
