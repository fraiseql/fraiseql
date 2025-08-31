"""FraiseQL Relay Entity Discovery

Automatic discovery and registration of entities from existing FraiseQL schemas
and PostgreSQL views/tables.
"""

from typing import Any, Dict, List, Optional, Type
from uuid import UUID

import fraiseql
from fraiseql import CQRSRepository


class EntityDiscovery:
    """Automatically discover entities that can be registered with Relay extension.

    This class scans existing PostgreSQL schemas and FraiseQL type definitions
    to identify entities that can benefit from Relay Global Object Identification.
    """

    def __init__(self, db_pool):
        self.db_pool = db_pool
        self.discovered_entities = []

    async def discover_from_database(self, schemas: List[str] = None) -> List[Dict[str, Any]]:
        """Discover entities by analyzing existing PostgreSQL views and tables.

        Args:
            schemas: List of schema names to scan (defaults to common patterns)

        Returns:
            List of entity information dictionaries
        """
        if schemas is None:
            schemas = ["public", "core"]  # Default schemas to scan

        entities = []

        async with self.db_pool.acquire() as conn:
            repo = CQRSRepository(conn)

            # Find views that follow FraiseQL patterns
            view_query = """
                SELECT schemaname, viewname, definition
                FROM pg_views
                WHERE schemaname = ANY($1)
                AND (viewname LIKE 'v_%' OR viewname LIKE 'tv_%' OR viewname LIKE 'mv_%')
                ORDER BY viewname
            """

            views = await repo.execute_raw(view_query, schemas)

            for view in views:
                entity_info = await self._analyze_view(repo, view)
                if entity_info:
                    entities.append(entity_info)

            # Find tables that follow command-side patterns
            table_query = """
                SELECT schemaname, tablename
                FROM pg_tables
                WHERE schemaname = ANY($1)
                AND tablename LIKE 'tb_%'
                ORDER BY tablename
            """

            tables = await repo.execute_raw(table_query, schemas)

            for table in tables:
                entity_info = await self._analyze_table(repo, table, entities)
                if entity_info:
                    # Update existing entity or add new one
                    existing = next(
                        (
                            e
                            for e in entities
                            if e["source_table"] == f"{table['schemaname']}.{table['tablename']}"
                        ),
                        None,
                    )
                    if existing:
                        existing.update(entity_info)
                    else:
                        entities.append(entity_info)

        self.discovered_entities = entities
        return entities

    async def discover_from_schema(self, schema) -> List[Dict[str, Any]]:
        """Discover entities from FraiseQL schema type definitions.

        Args:
            schema: FraiseQL GraphQL schema object

        Returns:
            List of entity information dictionaries
        """
        entities = []

        # Get all types from schema that might be entities
        if hasattr(schema, "_type_map"):
            type_map = schema._type_map
        elif hasattr(schema, "type_map"):
            type_map = schema.type_map
        else:
            # Try to get types through introspection
            type_map = {}

        for type_name, type_def in type_map.items():
            if self._is_potential_entity_type(type_def):
                entity_info = await self._analyze_graphql_type(type_def, type_name)
                if entity_info:
                    entities.append(entity_info)

        # Also check for classes with relay entity metadata
        entities.extend(await self._find_decorated_entities())

        return entities

    async def discover_and_register_all(self, relay_integration, schemas: List[str] = None) -> int:
        """Discover all entities and register them with the Relay integration.

        Args:
            relay_integration: RelayIntegration instance
            schemas: PostgreSQL schemas to scan

        Returns:
            Number of entities registered
        """
        # Discover from database
        db_entities = await self.discover_from_database(schemas)

        registered_count = 0
        for entity_info in db_entities:
            try:
                await relay_integration.register_entity_type(**entity_info)
                registered_count += 1
            except Exception as e:
                print(f"Warning: Could not register entity {entity_info.get('entity_name')}: {e}")

        return registered_count

    async def _analyze_view(
        self, repo: CQRSRepository, view: Dict[str, Any]
    ) -> Optional[Dict[str, Any]]:
        """Analyze a PostgreSQL view to extract entity information."""
        view_name = view["viewname"]
        schema_name = view["schemaname"]
        definition = view["definition"]

        # Determine entity type from view name
        if view_name.startswith("v_"):
            entity_name = self._view_name_to_entity_name(view_name[2:])  # Remove 'v_' prefix
            view_type = "v_table"
        elif view_name.startswith("tv_"):
            entity_name = self._view_name_to_entity_name(view_name[3:])  # Remove 'tv_' prefix
            view_type = "tv_table"
        elif view_name.startswith("mv_"):
            entity_name = self._view_name_to_entity_name(view_name[3:])  # Remove 'mv_' prefix
            view_type = "mv_table"
        else:
            return None

        # Try to find the primary key column by analyzing the view
        pk_column = await self._extract_pk_column(repo, f"{schema_name}.{view_name}", entity_name)

        if not pk_column:
            return None  # Can't create entity without primary key

        # Try to find corresponding source table
        source_table = await self._find_source_table(repo, entity_name)

        entity_info = {
            "entity_name": entity_name,
            "pk_column": pk_column,
            "source_table": source_table or f"tb_{entity_name.lower()}",
            view_type: f"{schema_name}.{view_name}",
        }

        # Set the main view table
        if view_type == "v_table":
            entity_info["v_table"] = f"{schema_name}.{view_name}"
        else:
            # For tv_/mv_ tables, try to find corresponding v_ view
            v_view = await self._find_corresponding_view(repo, entity_name, "v_")
            if v_view:
                entity_info["v_table"] = v_view

        return entity_info

    async def _analyze_table(
        self, repo: CQRSRepository, table: Dict[str, Any], existing_entities: List[Dict]
    ) -> Optional[Dict[str, Any]]:
        """Analyze a command-side table to extract entity information."""
        table_name = table["tablename"]
        schema_name = table["schemaname"]

        if not table_name.startswith("tb_"):
            return None

        entity_name = self._table_name_to_entity_name(table_name[3:])  # Remove 'tb_' prefix

        # Find UUID primary key column
        pk_query = """
            SELECT column_name
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            AND data_type = 'uuid'
            AND column_name LIKE 'pk_%'
            ORDER BY ordinal_position
            LIMIT 1
        """

        pk_result = await repo.execute_raw(pk_query, [schema_name, table_name])
        if not pk_result:
            return None

        pk_column = pk_result[0]["column_name"]

        return {
            "entity_name": entity_name,
            "pk_column": pk_column,
            "source_table": f"{schema_name}.{table_name}",
        }

    async def _analyze_graphql_type(
        self, type_def: Any, type_name: str
    ) -> Optional[Dict[str, Any]]:
        """Analyze a GraphQL type definition to extract entity information."""
        # Check if type has relay entity metadata (from decorator)
        if hasattr(type_def, "_relay_entity_info"):
            info = type_def._relay_entity_info.copy()
            info["python_type"] = type_def
            return info

        # Check if type implements Node interface
        if hasattr(type_def, "__annotations__") and "id" in type_def.__annotations__:
            # This might be a Node type - try to infer entity information
            entity_name = type_name

            return {
                "entity_name": entity_name,
                "python_type": type_def,
                "pk_column": f"pk_{entity_name.lower()}",
                "v_table": f"v_{entity_name.lower()}",
                "source_table": f"tb_{entity_name.lower()}",
            }

        return None

    async def _find_decorated_entities(self) -> List[Dict[str, Any]]:
        """Find classes decorated with @relay_entity."""
        # This would require more sophisticated reflection to find all
        # classes in the current module/package that have been decorated.
        # For now, return empty list and rely on explicit registration.
        return []

    def _is_potential_entity_type(self, type_def: Any) -> bool:
        """Check if a GraphQL type definition is potentially a Relay entity."""
        # Basic heuristics to identify entity types
        if not hasattr(type_def, "__annotations__"):
            return False

        annotations = type_def.__annotations__

        # Must have an 'id' field
        if "id" not in annotations:
            return False

        # Should have multiple fields (not just id)
        if len(annotations) < 2:
            return False

        # Should not be a built-in scalar or utility type
        type_name = getattr(type_def, "__name__", "")
        if type_name in ["String", "Int", "Float", "Boolean", "ID", "UUID"]:
            return False

        return True

    def _view_name_to_entity_name(self, view_suffix: str) -> str:
        """Convert view suffix to entity name (e.g., 'user' -> 'User')."""
        return self._snake_to_pascal_case(view_suffix)

    def _table_name_to_entity_name(self, table_suffix: str) -> str:
        """Convert table suffix to entity name (e.g., 'user' -> 'User')."""
        return self._snake_to_pascal_case(table_suffix)

    def _snake_to_pascal_case(self, snake_str: str) -> str:
        """Convert snake_case to PascalCase."""
        return "".join(word.capitalize() for word in snake_str.split("_"))

    async def _extract_pk_column(
        self, repo: CQRSRepository, view_name: str, entity_name: str
    ) -> Optional[str]:
        """Try to extract primary key column name from view definition or data."""
        # Common patterns for primary key columns
        possible_pk_names = [
            f"pk_{entity_name.lower()}",
            f"pk{entity_name.lower()}",
            "id",
            "pk",
        ]

        # Try to query the view to see what columns exist
        try:
            columns_query = f"""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = '{view_name.split(".")[-1]}'
                AND table_schema = '{view_name.split(".")[0]}'
                AND data_type = 'uuid'
            """

            columns = await repo.execute_raw(columns_query)

            for col in columns:
                col_name = col["column_name"]
                if col_name in possible_pk_names:
                    return col_name

            # If no exact match, return first UUID column
            if columns:
                return columns[0]["column_name"]

        except Exception:
            # Fallback to most likely name
            return f"pk_{entity_name.lower()}"

        return None

    async def _find_source_table(self, repo: CQRSRepository, entity_name: str) -> Optional[str]:
        """Find the corresponding source table for an entity."""
        possible_table_names = [
            f"tb_{entity_name.lower()}",
            f"tb{entity_name.lower()}",
            entity_name.lower(),
        ]

        for table_name in possible_table_names:
            try:
                # Check if table exists
                exists_query = """
                    SELECT 1 FROM information_schema.tables
                    WHERE table_name = $1
                    LIMIT 1
                """
                result = await repo.execute_raw(exists_query, [table_name])
                if result:
                    return table_name
            except Exception:
                continue

        return None

    async def _find_corresponding_view(
        self, repo: CQRSRepository, entity_name: str, prefix: str
    ) -> Optional[str]:
        """Find a corresponding view with a different prefix."""
        view_name = f"{prefix}{entity_name.lower()}"

        try:
            exists_query = """
                SELECT schemaname FROM pg_views
                WHERE viewname = $1
                LIMIT 1
            """
            result = await repo.execute_raw(exists_query, [view_name])
            if result:
                schema = result[0]["schemaname"]
                return f"{schema}.{view_name}"
        except Exception:
            pass

        return None


async def discover_and_register_entities(
    relay_integration, schemas: List[str] = None, include_schema_types: bool = True
) -> int:
    """Convenience function to discover and register all entities.

    Args:
        relay_integration: RelayIntegration instance
        schemas: PostgreSQL schemas to scan
        include_schema_types: Whether to also scan GraphQL schema types

    Returns:
        Number of entities registered
    """
    discovery = EntityDiscovery(relay_integration.db_pool)

    total_registered = 0

    # Discover from database
    db_entities = await discovery.discover_from_database(schemas)
    for entity_info in db_entities:
        try:
            # Create a dummy Python type if none provided
            if "python_type" not in entity_info:
                entity_info["python_type"] = create_dynamic_node_type(entity_info["entity_name"])

            await relay_integration.register_entity_type(**entity_info)
            total_registered += 1
        except Exception as e:
            print(f"Warning: Could not register entity {entity_info.get('entity_name')}: {e}")

    return total_registered


def create_dynamic_node_type(entity_name: str) -> Type:
    """Create a dynamic Node type for entities discovered from the database.

    This creates a minimal Python class that can be used for node resolution
    when no explicit Python type is available.
    """

    @fraiseql.type
    class DynamicNode:
        id: UUID

        @classmethod
        def from_dict(cls, data: Dict[str, Any]) -> "DynamicNode":
            instance = cls()
            for key, value in data.items():
                setattr(instance, key, value)
            return instance

        def __init__(self):
            self.__typename = entity_name

    # Set the class name dynamically
    DynamicNode.__name__ = entity_name
    DynamicNode.__qualname__ = entity_name

    return DynamicNode
