"""Query complexity analysis for FraiseQL.

Analyzes GraphQL queries to determine their complexity, which is useful for:
- TurboRouter cache management
- Rate limiting based on query cost
- Performance monitoring and optimization
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from graphql import (
    DocumentNode,
    FieldNode,
    FragmentDefinitionNode,
    FragmentSpreadNode,
    InlineFragmentNode,
    OperationDefinitionNode,
    SelectionSetNode,
    parse,
)
from graphql.language import Visitor, visit

from fraiseql.analysis.complexity_config import ComplexityConfig

if TYPE_CHECKING:
    from graphql import GraphQLSchema


@dataclass
class ComplexityScore:
    """Represents the complexity score of a GraphQL query."""

    # Base complexity (number of fields)
    field_count: int = 0

    # Depth of nesting
    max_depth: int = 0

    # Number of array fields (potential for large result sets)
    array_field_count: int = 0

    # Number of unique types accessed
    type_diversity: int = 0

    # Fragment usage (reusable parts)
    fragment_count: int = 0

    # Calculated scores
    depth_score: int = 0
    array_score: int = 0

    @property
    def total_score(self) -> int:
        """Calculate total complexity score.

        Formula considers:
        - Each field adds base cost
        - Each level of depth multiplies by depth level
        - Array fields multiply by potential size factor
        - Type diversity adds overhead
        """
        config = ComplexityConfig.get_default()

        base = self.field_count
        depth_penalty = self.depth_score
        array_penalty = self.array_score
        type_penalty = self.type_diversity * config.type_diversity_cost

        return base + depth_penalty + array_penalty + type_penalty

    @property
    def cache_weight(self) -> float:
        """Calculate cache weight for TurboRouter.

        Returns a weight between 0.1 and 10.0 where:
        - < 1.0: Simple query, good for caching
        - 1.0-3.0: Moderate complexity
        - > 3.0: Complex query, consider not caching
        """
        # Use the config to calculate weight
        config = ComplexityConfig.get_default()
        return config.get_cache_weight(self.total_score)

    def should_cache(self, threshold: int = 200) -> bool:
        """Determine if query should be cached in TurboRouter.

        Args:
            threshold: Maximum complexity score for caching

        Returns:
            True if query should be cached
        """
        return self.total_score <= threshold


class QueryComplexityAnalyzer(Visitor):
    """Analyzes GraphQL query complexity by visiting AST nodes."""

    def __init__(
        self,
        schema: GraphQLSchema | None = None,
        config: ComplexityConfig | None = None,
    ) -> None:
        """Initialize the analyzer.

        Args:
            schema: Optional GraphQL schema for type information
            config: Complexity configuration (uses default if None)
        """
        super().__init__()  # Initialize parent Visitor
        self.schema = schema
        self.config = config or ComplexityConfig.get_default()
        self.score = ComplexityScore()
        self.current_depth = 0
        self.types_accessed: set[str] = set()
        self.fragments: dict[str, FragmentDefinitionNode] = {}

    def analyze(self, query: str | DocumentNode) -> ComplexityScore:
        """Analyze a GraphQL query and return its complexity score.

        Args:
            query: GraphQL query string or parsed document

        Returns:
            ComplexityScore with analysis results
        """
        # Parse if string
        document = parse(query) if isinstance(query, str) else query

        # Reset state
        self.score = ComplexityScore()
        self.current_depth = 0
        self.types_accessed.clear()
        self.fragments.clear()

        # Visit the document
        visit(document, self)

        # Calculate final scores
        self.score.type_diversity = len(self.types_accessed)

        return self.score

    def enter_operation_definition(self, node: OperationDefinitionNode, *_) -> None:
        """Enter an operation definition."""
        # Track operation type
        if node.operation.value in ("query", "mutation", "subscription"):
            self.types_accessed.add(node.operation.value.capitalize())

    def enter_fragment_definition(self, node: FragmentDefinitionNode, *_) -> None:
        """Enter a fragment definition."""
        self.fragments[node.name.value] = node
        self.score.fragment_count += 1

    def enter_field(self, node: FieldNode, *_) -> None:
        """Enter a field selection."""
        self.score.field_count += self.config.base_field_cost

        # Track field name patterns that suggest arrays
        field_name = node.name.value
        if self.config.is_array_field(field_name):
            self.score.array_field_count += 1
            self.score.array_score += self.config.calculate_array_penalty(self.current_depth, 1)

        # Add depth score - using config's penalty calculation
        self.score.depth_score += self.config.calculate_depth_penalty(self.current_depth)

    def enter_selection_set(self, node: SelectionSetNode, *_) -> None:
        """Enter a selection set (nested fields)."""
        self.current_depth += 1
        self.score.max_depth = max(self.score.max_depth, self.current_depth)

    def leave_selection_set(self, node: SelectionSetNode, *_) -> None:
        """Leave a selection set."""
        self.current_depth -= 1

    def enter_fragment_spread(self, node: FragmentSpreadNode, *_) -> None:
        """Enter a fragment spread."""
        # Analyze the fragment if we have it
        fragment_name = node.name.value
        if fragment_name in self.fragments:
            # This is simplified - in production we'd properly handle recursive fragments
            pass

    def enter_inline_fragment(self, node: InlineFragmentNode, *_) -> None:
        """Enter an inline fragment."""
        if node.type_condition:
            self.types_accessed.add(node.type_condition.name.value)


def analyze_query_complexity(
    query: str,
    schema: GraphQLSchema | None = None,
    config: ComplexityConfig | None = None,
) -> ComplexityScore:
    """Analyze the complexity of a GraphQL query.

    Args:
        query: GraphQL query string
        schema: Optional GraphQL schema for enhanced analysis
        config: Complexity configuration (uses default if None)

    Returns:
        ComplexityScore with analysis results
    """
    analyzer = QueryComplexityAnalyzer(schema, config)
    return analyzer.analyze(query)


def should_cache_query(
    query: str,
    schema: GraphQLSchema | None = None,
    complexity_threshold: int | None = None,
    config: ComplexityConfig | None = None,
) -> tuple[bool, ComplexityScore]:
    """Determine if a query should be cached in TurboRouter.

    Args:
        query: GraphQL query string
        schema: Optional GraphQL schema
        complexity_threshold: Maximum complexity for caching (uses config default if None)
        config: Complexity configuration (uses default if None)

    Returns:
        Tuple of (should_cache, complexity_score)
    """
    config = config or ComplexityConfig.get_default()
    threshold = complexity_threshold or config.complex_query_threshold
    score = analyze_query_complexity(query, schema, config)
    return score.should_cache(threshold), score


def calculate_cache_weight(
    query: str,
    schema: GraphQLSchema | None = None,
    config: ComplexityConfig | None = None,
) -> float:
    """Calculate the cache weight for a query.

    Args:
        query: GraphQL query string
        schema: Optional GraphQL schema
        config: Complexity configuration (uses default if None)

    Returns:
        Cache weight (0.1 to 10.0)
    """
    score = analyze_query_complexity(query, schema, config)
    return score.cache_weight
