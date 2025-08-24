"""GraphQL query and mutation builders for FraiseQL testing.

This module provides fluent builders for constructing GraphQL queries,
mutations, and subscriptions with proper syntax, variable handling,
fragments, and common patterns used in FraiseQL applications.
"""

import json
from typing import Any, Dict, List, Optional


class GraphQLQueryBuilder:
    """Fluent builder for GraphQL queries."""

    def __init__(self):
        """Initialize query builder."""
        self.reset()

    def reset(self) -> "GraphQLQueryBuilder":
        """Reset builder state."""
        self._operation_type = "query"
        self._operation_name: Optional[str] = None
        self._variables: Dict[str, str] = {}
        self._fields: List[str] = []
        self._fragments: List[str] = []
        return self

    def query(self, name: Optional[str] = None) -> "GraphQLQueryBuilder":
        """Start a query operation.

        Args:
            name: Optional operation name

        Returns:
            Self for method chaining
        """
        self._operation_type = "query"
        self._operation_name = name
        return self

    def mutation(self, name: Optional[str] = None) -> "GraphQLQueryBuilder":
        """Start a mutation operation.

        Args:
            name: Optional operation name

        Returns:
            Self for method chaining
        """
        self._operation_type = "mutation"
        self._operation_name = name
        return self

    def subscription(self, name: Optional[str] = None) -> "GraphQLQueryBuilder":
        """Start a subscription operation.

        Args:
            name: Optional operation name

        Returns:
            Self for method chaining
        """
        self._operation_type = "subscription"
        self._operation_name = name
        return self

    def variable(self, name: str, type_def: str) -> "GraphQLQueryBuilder":
        """Add a variable definition.

        Args:
            name: Variable name (without $)
            type_def: GraphQL type (e.g., "String!", "[ID!]!")

        Returns:
            Self for method chaining
        """
        self._variables[name] = type_def
        return self

    def field(
        self,
        name: str,
        alias: Optional[str] = None,
        args: Optional[Dict[str, Any]] = None,
        fields: Optional[List[str]] = None,
    ) -> "GraphQLQueryBuilder":
        """Add a field to the query.

        Args:
            name: Field name
            alias: Optional field alias
            args: Field arguments
            fields: Nested fields (for object types)

        Returns:
            Self for method chaining
        """
        field_parts = []

        # Add alias if provided
        if alias:
            field_parts.append(f"{alias}: ")

        # Add field name
        field_parts.append(name)

        # Add arguments if provided
        if args:
            arg_strings = []
            for key, value in args.items():
                arg_strings.append(f"{key}: {self._format_value(value)}")
            field_parts.append(f"({', '.join(arg_strings)})")

        # Add nested fields if provided
        if fields:
            nested = self._format_nested_fields(fields)
            field_parts.append(f" {nested}")

        self._fields.append("".join(field_parts))
        return self

    def nested_field(
        self,
        name: str,
        fields: List[str],
        alias: Optional[str] = None,
        args: Optional[Dict[str, Any]] = None,
    ) -> "GraphQLQueryBuilder":
        """Add a nested field with subfields.

        Args:
            name: Field name
            fields: List of subfield names or nested structures
            alias: Optional field alias
            args: Field arguments

        Returns:
            Self for method chaining
        """
        return self.field(name, alias, args, fields)

    def fragment(self, name: str, type_name: str, fields: List[str]) -> "GraphQLQueryBuilder":
        """Add a fragment definition.

        Args:
            name: Fragment name
            type_name: GraphQL type name
            fields: List of fields in fragment

        Returns:
            Self for method chaining
        """
        fragment_body = self._format_nested_fields(fields)
        fragment = f"fragment {name} on {type_name} {fragment_body}"
        self._fragments.append(fragment)
        return self

    def inline_fragment(self, type_name: str, fields: List[str]) -> "GraphQLQueryBuilder":
        """Add an inline fragment.

        Args:
            type_name: GraphQL type name
            fields: List of fields in fragment

        Returns:
            Self for method chaining
        """
        fragment_body = self._format_nested_fields(fields, indent_level=1)
        inline_fragment = f"... on {type_name} {fragment_body}"
        self._fields.append(inline_fragment)
        return self

    def fragment_spread(self, fragment_name: str) -> "GraphQLQueryBuilder":
        """Add a fragment spread.

        Args:
            fragment_name: Name of fragment to spread

        Returns:
            Self for method chaining
        """
        self._fields.append(f"...{fragment_name}")
        return self

    def build(self) -> str:
        """Build the final GraphQL query string.

        Returns:
            str: Complete GraphQL query
        """
        query_parts = []

        # Add fragments first
        if self._fragments:
            query_parts.extend(self._fragments)
            query_parts.append("")  # Empty line separator

        # Build operation header
        operation_parts = [self._operation_type]

        if self._operation_name:
            operation_parts.append(f" {self._operation_name}")

        # Add variables if any
        if self._variables:
            var_defs = []
            for name, type_def in self._variables.items():
                var_defs.append(f"${name}: {type_def}")
            operation_parts.append(f"({', '.join(var_defs)})")

        # Add operation body
        operation_parts.append(" {\n")

        # Add fields with proper indentation
        for field in self._fields:
            # Add indentation to each line of the field
            field_lines = field.split("\n")
            indented_lines = []
            for line in field_lines:
                if line.strip():  # Only indent non-empty lines
                    indented_lines.append(f"  {line}")
                else:
                    indented_lines.append(line)
            operation_parts.append("\n".join(indented_lines))
            operation_parts.append("\n")

        operation_parts.append("}")

        query_parts.append("".join(operation_parts))

        return "\n\n".join(query_parts)

    def _format_value(self, value: Any) -> str:
        """Format a value for GraphQL syntax.

        Args:
            value: Python value

        Returns:
            str: GraphQL formatted value
        """
        if isinstance(value, str):
            if value.startswith("$"):
                # Variable reference
                return value
            # String literal
            return json.dumps(value)
        if isinstance(value, bool):
            return "true" if value else "false"
        if value is None:
            return "null"
        if isinstance(value, (int, float)):
            return str(value)
        if isinstance(value, list):
            formatted_items = [self._format_value(item) for item in value]
            return f"[{', '.join(formatted_items)}]"
        if isinstance(value, dict):
            formatted_pairs = []
            for k, v in value.items():
                formatted_pairs.append(f"{k}: {self._format_value(v)}")
            return f"{{{', '.join(formatted_pairs)}}}"
        return str(value)

    def _format_nested_fields(self, fields: List[str], indent_level: int = 0) -> str:
        """Format nested fields with proper indentation.

        Args:
            fields: List of field strings
            indent_level: Current indentation level

        Returns:
            str: Formatted field block
        """
        if not fields:
            return ""

        indent = "  " * indent_level
        nested_indent = "  " * (indent_level + 1)

        formatted_fields = []
        for field in fields:
            # Handle nested field structures
            if isinstance(field, dict):
                # Format as nested object
                for field_name, subfields in field.items():
                    if isinstance(subfields, list):
                        nested = self._format_nested_fields(subfields, indent_level + 2)
                        formatted_fields.append(f"{nested_indent}{field_name} {nested}")
                    else:
                        formatted_fields.append(f"{nested_indent}{field_name}")
            else:
                # Simple field name
                formatted_fields.append(f"{nested_indent}{field}")

        return "{\n" + "\n".join(formatted_fields) + f"\n{indent}}}"


class MutationBuilder(GraphQLQueryBuilder):
    """Specialized builder for GraphQL mutations."""

    def __init__(self):
        """Initialize mutation builder."""
        super().__init__()
        self.mutation()

    def input_field(
        self,
        mutation_name: str,
        input_var: str = "input",
        result_fields: Optional[List[str]] = None,
    ) -> "MutationBuilder":
        """Add a mutation field with input variable.

        Args:
            mutation_name: Name of the mutation
            input_var: Variable name for input (default: "input")
            result_fields: Fields to select from result

        Returns:
            Self for method chaining
        """
        args = {input_var: f"${input_var}"}

        if result_fields is None:
            result_fields = ["__typename"]

        return self.field(mutation_name, args=args, fields=result_fields)

    def union_result(
        self,
        mutation_name: str,
        success_type: str,
        success_fields: List[str],
        error_type: str,
        error_fields: Optional[List[str]] = None,
        input_var: str = "input",
    ) -> "MutationBuilder":
        """Add mutation with union result type (success/error pattern).

        Args:
            mutation_name: Name of the mutation
            success_type: Success result type name
            success_fields: Fields for success case
            error_type: Error result type name
            error_fields: Fields for error case (default: ["message", "code"])
            input_var: Variable name for input

        Returns:
            Self for method chaining
        """
        if error_fields is None:
            error_fields = ["message", "code"]

        result_fields = [
            "__typename",
        ]

        # Add inline fragments for union types
        self.field(mutation_name, args={input_var: f"${input_var}"}, fields=result_fields)
        self.inline_fragment(success_type, success_fields)
        self.inline_fragment(error_type, error_fields)

        return self


class QueryBuilder(GraphQLQueryBuilder):
    """Specialized builder for GraphQL queries."""

    def __init__(self):
        """Initialize query builder."""
        super().__init__()
        self.query()

    def list_query(
        self, field_name: str, item_fields: List[str], args: Optional[Dict[str, Any]] = None
    ) -> "QueryBuilder":
        """Add a list query with common pagination arguments.

        Args:
            field_name: Name of the list field
            item_fields: Fields to select for each item
            args: Additional query arguments

        Returns:
            Self for method chaining
        """
        query_args = args or {}

        return self.field(field_name, args=query_args, fields=item_fields)

    def connection_query(
        self,
        field_name: str,
        node_fields: List[str],
        args: Optional[Dict[str, Any]] = None,
        page_info: bool = True,
    ) -> "QueryBuilder":
        """Add a Relay connection query.

        Args:
            field_name: Name of the connection field
            node_fields: Fields to select for each node
            args: Connection arguments (first, after, etc.)
            page_info: Whether to include pageInfo

        Returns:
            Self for method chaining
        """
        connection_fields = [{"edges": ["cursor", {"node": node_fields}]}]

        if page_info:
            connection_fields.append(
                {"pageInfo": ["hasNextPage", "hasPreviousPage", "startCursor", "endCursor"]}
            )

        return self.field(field_name, args=args, fields=connection_fields)

    def single_query(
        self, field_name: str, fields: List[str], args: Optional[Dict[str, Any]] = None
    ) -> "QueryBuilder":
        """Add a single item query.

        Args:
            field_name: Name of the query field
            fields: Fields to select
            args: Query arguments (usually ID)

        Returns:
            Self for method chaining
        """
        return self.field(field_name, args=args, fields=fields)


# Predefined query patterns
def build_user_query(
    user_id: str, include_profile: bool = True, include_posts: bool = False
) -> str:
    """Build a user query with common fields.

    Args:
        user_id: User ID to query
        include_profile: Whether to include profile fields
        include_posts: Whether to include user's posts

    Returns:
        str: GraphQL query
    """
    builder = QueryBuilder()
    builder.variable("id", "ID!")

    fields = ["id", "username", "email", "createdAt"]

    if include_profile:
        fields.extend([{"profile": ["bio", "avatarUrl", "website", "location"]}])

    if include_posts:
        fields.extend([{"posts": ["id", "title", "slug", "publishedAt", "status"]}])

    builder.single_query("user", fields, {"id": "$id"})

    return builder.build()


def build_posts_list_query(
    limit: int = 10, include_author: bool = True, include_comments_count: bool = False
) -> str:
    """Build a posts list query.

    Args:
        limit: Number of posts to fetch
        include_author: Whether to include author info
        include_comments_count: Whether to include comment count

    Returns:
        str: GraphQL query
    """
    builder = QueryBuilder()
    builder.variable("limit", "Int")
    builder.variable("offset", "Int")

    fields = ["id", "title", "slug", "excerpt", "publishedAt", "status"]

    if include_author:
        fields.extend([{"author": ["id", "username", "email"]}])

    if include_comments_count:
        fields.append("commentCount")

    args = {"limit": "$limit", "offset": "$offset"}
    builder.list_query("posts", fields, args)

    return builder.build()


def build_create_post_mutation(include_validation_errors: bool = True) -> str:
    """Build a create post mutation.

    Args:
        include_validation_errors: Whether to include validation error fields

    Returns:
        str: GraphQL mutation
    """
    builder = MutationBuilder()
    builder.variable("input", "CreatePostInput!")

    success_fields = ["message", {"post": ["id", "title", "slug", "status", "createdAt"]}]

    error_fields = ["message", "code"]
    if include_validation_errors:
        error_fields.append("validationErrors")

    builder.union_result(
        "createPost", "CreatePostSuccess", success_fields, "CreatePostError", error_fields
    )

    return builder.build()


# Utility functions for common patterns
def introspection_query() -> str:
    """Build schema introspection query.

    Returns:
        str: GraphQL introspection query
    """
    return """
    query IntrospectionQuery {
      __schema {
        queryType { name }
        mutationType { name }
        subscriptionType { name }
        types {
          ...FullType
        }
        directives {
          name
          description
          locations
          args {
            ...InputValue
          }
        }
      }
    }

    fragment FullType on __Type {
      kind
      name
      description
      fields(includeDeprecated: true) {
        name
        description
        args {
          ...InputValue
        }
        type {
          ...TypeRef
        }
        isDeprecated
        deprecationReason
      }
      inputFields {
        ...InputValue
      }
      interfaces {
        ...TypeRef
      }
      enumValues(includeDeprecated: true) {
        name
        description
        isDeprecated
        deprecationReason
      }
      possibleTypes {
        ...TypeRef
      }
    }

    fragment InputValue on __InputValue {
      name
      description
      type { ...TypeRef }
      defaultValue
    }

    fragment TypeRef on __Type {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
    }
    """


def typename_query(type_name: str) -> str:
    """Build a simple __typename query.

    Args:
        type_name: GraphQL type to query

    Returns:
        str: GraphQL query
    """
    return f"""
    query {{
      {type_name} {{
        __typename
      }}
    }}
    """
