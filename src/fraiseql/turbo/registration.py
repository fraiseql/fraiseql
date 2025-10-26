"""TurboRouter query registration system."""

from dataclasses import dataclass
from typing import Callable, Optional

from graphql import DocumentNode, OperationDefinitionNode, parse

from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry
from fraiseql.gql.schema_builder import SchemaRegistry


@dataclass
class RegistrationResult:
    """Result of query registration attempt."""

    success: bool
    query_hash: Optional[str] = None
    error: Optional[str] = None
    sql_template: Optional[str] = None
    param_mapping: Optional[dict[str, str]] = None


class TurboRegistration:
    """Handles registration of queries for TurboRouter optimization."""

    def __init__(self, registry: TurboRegistry) -> None:
        """Initialize registration system.

        Args:
            registry: TurboRegistry instance for storing queries
        """
        self.registry = registry
        from fraiseql.turbo.sql_compiler import SQLCompiler

        self.sql_compiler = SQLCompiler()
        self.schema_registry = SchemaRegistry.get_instance()

    def register_from_resolver(
        self,
        resolver_func: Callable,
        query_template: Optional[str] = None,
        operation_name: Optional[str] = None,
    ) -> RegistrationResult:
        """Register a query from a resolver function.

        Args:
            resolver_func: The resolver function to analyze
            query_template: Optional GraphQL query template
            operation_name: Optional operation name

        Returns:
            RegistrationResult with success status and details
        """
        try:
            # Extract function metadata
            func_name = resolver_func.__name__
            if operation_name is None:
                operation_name = self._generate_operation_name(func_name)

            # Extract SQL pattern from resolver
            sql_info = self.sql_compiler.extract_from_resolver(resolver_func)
            if not sql_info:
                return RegistrationResult(
                    success=False, error="Could not extract SQL pattern from resolver"
                )

            # Generate or validate GraphQL query
            if query_template:
                graphql_query = query_template
            else:
                # Try to infer from registered schema
                graphql_query = self._infer_graphql_query(func_name)
                if not graphql_query:
                    return RegistrationResult(
                        success=False, error="Could not infer GraphQL query structure"
                    )

            # Create TurboQuery
            turbo_query = TurboQuery(
                graphql_query=graphql_query,
                sql_template=sql_info.sql_template,
                param_mapping=sql_info.param_mapping,
                operation_name=operation_name,
            )

            # Register with registry
            query_hash = self.registry.register(turbo_query)

            return RegistrationResult(
                success=True,
                query_hash=query_hash,
                sql_template=sql_info.sql_template,
                param_mapping=sql_info.param_mapping,
            )

        except Exception as e:
            return RegistrationResult(success=False, error=f"Registration failed: {e!s}")

    def register_from_graphql(
        self, query: str, view_mapping: dict[str, str], operation_name: Optional[str] = None
    ) -> RegistrationResult:
        """Register a query from GraphQL string.

        Args:
            query: GraphQL query string
            view_mapping: Mapping of GraphQL types to database views
            operation_name: Optional operation name

        Returns:
            RegistrationResult with success status and details
        """
        try:
            # Parse GraphQL query
            document = parse(query)

            # Extract operation
            operation = self._get_operation(document)
            if not operation:
                return RegistrationResult(success=False, error="No operation found in query")

            # Generate SQL template
            sql_info = self.sql_compiler.compile_from_graphql(document, view_mapping)

            # Use provided operation name or extract from query
            if not operation_name and operation.name:
                operation_name = operation.name.value

            # Create TurboQuery
            turbo_query = TurboQuery(
                graphql_query=query,
                sql_template=sql_info.sql_template,
                param_mapping=sql_info.param_mapping,
                operation_name=operation_name,
            )

            # Register with registry
            query_hash = self.registry.register(turbo_query)

            return RegistrationResult(
                success=True,
                query_hash=query_hash,
                sql_template=sql_info.sql_template,
                param_mapping=sql_info.param_mapping,
            )

        except Exception as e:
            return RegistrationResult(success=False, error=f"Registration failed: {e!s}")

    def _generate_operation_name(self, func_name: str) -> str:
        """Generate operation name from function name."""
        # Convert snake_case to PascalCase
        parts = func_name.split("_")
        return "".join(word.capitalize() for word in parts)

    def _infer_graphql_query(self, func_name: str) -> Optional[str]:
        """Try to infer GraphQL query structure from function name."""
        # This is a simplified implementation
        # In production, this would use schema introspection
        return None

    def _get_operation(self, document: DocumentNode) -> Optional[OperationDefinitionNode]:
        """Extract the first operation from a GraphQL document."""
        for definition in document.definitions:
            if isinstance(definition, OperationDefinitionNode):
                return definition
        return None
