"""GraphQL query complexity analysis and limiting.

This module provides functionality to analyze GraphQL query complexity
and prevent resource-intensive queries from being executed.
"""

from dataclasses import dataclass, field
from typing import Any, Dict, Optional

from graphql import (
    FieldNode,
    FragmentDefinitionNode,
    FragmentSpreadNode,
    GraphQLError,
    OperationDefinitionNode,
    SelectionNode,
    parse,
)

from fraiseql.audit import get_security_logger
from fraiseql.audit.security_logger import SecurityEvent, SecurityEventSeverity, SecurityEventType


class ComplexityError(GraphQLError):
    """Raised when query complexity exceeds limits."""

    def __init__(
        self,
        message: str,
        complexity: Optional[int] = None,
        depth: Optional[int] = None,
        limit: Optional[int] = None,
    ):
        """Initialize complexity error."""
        super().__init__(message)
        self.complexity = complexity
        self.depth = depth
        self.limit = limit


@dataclass
class ComplexityConfig:
    """Configuration for query complexity analysis."""

    # Maximum allowed complexity score
    max_complexity: int = 1000

    # Maximum allowed query depth
    max_depth: int = 10

    # Default size estimate for lists without explicit limit
    default_list_size: int = 10

    # Custom multipliers for expensive fields
    field_multipliers: Dict[str, int] = field(default_factory=dict)

    # Whether complexity checking is enabled
    enabled: bool = True

    # Include complexity info in response extensions
    include_in_response: bool = False

    # Allow introspection queries
    allow_introspection: bool = True


@dataclass
class ComplexityInfo:
    """Information about query complexity."""

    total_score: int
    depth: int
    field_count: int
    field_scores: Dict[str, int] = field(default_factory=dict)


def calculate_field_complexity(
    field_name: str,
    *,
    is_list: bool = False,
    is_object: bool = False,
    limit: Optional[int] = None,
    estimated_size: int = 10,
    nested_complexity: int = 0,
    multiplier: int = 1,
) -> int:
    """Calculate complexity score for a single field.

    Args:
        field_name: Name of the field
        is_list: Whether field returns a list
        is_object: Whether field returns an object
        limit: Explicit limit for list fields
        estimated_size: Estimated size for lists without limit
        nested_complexity: Complexity of nested fields
        multiplier: Custom multiplier for expensive fields

    Returns:
        Complexity score for the field
    """
    # Base complexity
    base_complexity = 1

    # Apply custom multiplier to base
    base_complexity *= multiplier

    # For lists, multiply the result by list size
    if is_list:
        list_size = limit if limit is not None else estimated_size
        # Base + (size * nested)
        return base_complexity + (list_size * nested_complexity)

    # For objects, just add nested complexity
    if is_object:
        return base_complexity + nested_complexity

    # For scalars, return base
    return base_complexity


def calculate_query_complexity(
    node: SelectionNode,
    fragments: Dict[str, FragmentDefinitionNode],
    config: ComplexityConfig,
    variables: Optional[Dict[str, Any]] = None,
    depth: int = 0,
) -> tuple[int, int, int]:
    """Calculate complexity for a selection node.

    Args:
        node: Selection node to analyze
        fragments: Available fragment definitions
        config: Complexity configuration
        variables: Query variables
        depth: Current depth in query

    Returns:
        Tuple of (complexity_score, max_depth, field_count)
    """
    if depth > config.max_depth:
        raise ComplexityError(
            f"Query exceeds maximum depth of {config.max_depth}",
            depth=depth,
            limit=config.max_depth,
        )

    if isinstance(node, FieldNode):
        field_name = node.name.value

        # Check for introspection
        if field_name.startswith("__") and not config.allow_introspection:
            raise ComplexityError("Introspection queries are not allowed")

        # Get custom multiplier
        multiplier = config.field_multipliers.get(field_name, 1)

        # Check if it's a list field (would need schema info in real implementation)
        # For now, assume fields ending with 's' are lists
        is_list = field_name.endswith("s") or field_name in ["search", "items", "results"]

        # Get limit from arguments
        limit = None
        if node.arguments:
            for arg in node.arguments:
                if arg.name.value == "limit":
                    if hasattr(arg.value, "value"):
                        limit = int(arg.value.value)
                    elif (
                        hasattr(arg.value, "name")
                        and variables
                        and arg.value.name.value in variables
                    ):
                        limit = variables[arg.value.name.value]

        # Calculate nested complexity
        nested_complexity = 0
        max_nested_depth = depth + 1
        nested_field_count = 0

        if node.selection_set:
            for selection in node.selection_set.selections:
                complexity, node_depth, fields = calculate_query_complexity(
                    selection,
                    fragments,
                    config,
                    variables,
                    depth + 1,
                )
                nested_complexity += complexity
                max_nested_depth = max(max_nested_depth, node_depth)
                nested_field_count += fields

        # Calculate this field's complexity
        field_complexity = calculate_field_complexity(
            field_name,
            is_list=is_list,
            is_object=bool(node.selection_set),
            limit=limit,
            estimated_size=config.default_list_size,
            nested_complexity=nested_complexity,
            multiplier=multiplier,
        )

        return field_complexity, max_nested_depth, 1 + nested_field_count

    if isinstance(node, FragmentSpreadNode):
        fragment_name = node.name.value
        if fragment_name in fragments:
            fragment = fragments[fragment_name]
            total_complexity = 0
            max_depth = depth
            total_fields = 0

            for selection in fragment.selection_set.selections:
                complexity, node_depth, fields = calculate_query_complexity(
                    selection,
                    fragments,
                    config,
                    variables,
                    depth,
                )
                total_complexity += complexity
                max_depth = max(max_depth, node_depth)
                total_fields += fields

            return total_complexity, max_depth, total_fields

    return 0, depth, 0


class QueryComplexityAnalyzer:
    """Analyzer for GraphQL query complexity."""

    def __init__(self, config: ComplexityConfig):
        """Initialize analyzer with configuration."""
        self.config = config

    def analyze(self, query: str, variables: Optional[Dict[str, Any]] = None) -> ComplexityInfo:
        """Analyze query complexity.

        Args:
            query: GraphQL query string
            variables: Query variables

        Returns:
            ComplexityInfo with analysis results

        Raises:
            ComplexityError: If query exceeds configured limits
        """
        try:
            document = parse(query)
        except Exception as e:
            raise GraphQLError(f"Failed to parse query: {e}")

        # Extract fragments
        fragments = {}
        operations = []

        for definition in document.definitions:
            if isinstance(definition, FragmentDefinitionNode):
                fragments[definition.name.value] = definition
            elif isinstance(definition, OperationDefinitionNode):
                operations.append(definition)

        if not operations:
            raise GraphQLError("No operation found in query")

        # Analyze first operation (typically queries have one operation)
        operation = operations[0]

        total_complexity = 0
        max_depth = 0
        total_fields = 0
        field_scores = {}

        for selection in operation.selection_set.selections:
            complexity, depth, fields = calculate_query_complexity(
                selection,
                fragments,
                self.config,
                variables,
            )

            if isinstance(selection, FieldNode):
                field_name = selection.name.value
                field_scores[field_name] = complexity

            total_complexity += complexity
            max_depth = max(max_depth, depth)
            total_fields += fields

        # Check limits if enabled
        if self.config.enabled and total_complexity > self.config.max_complexity:
            # Log security event
            security_logger = get_security_logger()
            security_logger.log_event(
                SecurityEvent(
                    event_type=SecurityEventType.QUERY_COMPLEXITY_EXCEEDED,
                    severity=SecurityEventSeverity.WARNING,
                    metadata={
                        "complexity": total_complexity,
                        "limit": self.config.max_complexity,
                        "query": query[:200],  # First 200 chars
                    },
                ),
            )

            raise ComplexityError(
                f"Query complexity {total_complexity} exceeds maximum complexity {self.config.max_complexity}",
                complexity=total_complexity,
                limit=self.config.max_complexity,
            )

        if self.config.enabled and max_depth > self.config.max_depth:
            # Log security event
            security_logger = get_security_logger()
            security_logger.log_event(
                SecurityEvent(
                    event_type=SecurityEventType.QUERY_DEPTH_EXCEEDED,
                    severity=SecurityEventSeverity.WARNING,
                    metadata={
                        "depth": max_depth,
                        "limit": self.config.max_depth,
                        "query": query[:200],
                    },
                ),
            )

            raise ComplexityError(
                f"Query depth {max_depth} exceeds maximum depth {self.config.max_depth}",
                depth=max_depth,
                limit=self.config.max_depth,
            )

        return ComplexityInfo(
            total_score=total_complexity,
            depth=max_depth,
            field_count=total_fields,
            field_scores=field_scores,
        )
