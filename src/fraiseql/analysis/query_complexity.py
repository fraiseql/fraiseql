"""Query complexity analysis for FraiseQL.

Analyzes GraphQL queries to determine their complexity, which is useful for:
- TurboRouter cache management
- Rate limiting based on query cost
- Performance monitoring and optimization
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

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
        - Each field adds 1 point
        - Each level of depth multiplies by depth level
        - Array fields multiply by potential size factor
        - Type diversity adds overhead
        """
        base = self.field_count
        depth_penalty = self.depth_score
        array_penalty = self.array_score
        type_penalty = self.type_diversity * 2
        
        return base + depth_penalty + array_penalty + type_penalty
    
    @property
    def cache_weight(self) -> float:
        """Calculate cache weight for TurboRouter.
        
        Returns a weight between 0.1 and 10.0 where:
        - < 1.0: Simple query, good for caching
        - 1.0-3.0: Moderate complexity
        - > 3.0: Complex query, consider not caching
        """
        score = self.total_score
        
        if score < 10:
            return 0.1
        elif score < 25:
            return 0.5
        elif score < 50:
            return 1.0
        elif score < 100:
            return 2.0
        elif score < 200:
            return 3.0
        elif score < 500:
            return 5.0
        else:
            return 10.0
    
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
    
    def __init__(self, schema: GraphQLSchema | None = None) -> None:
        """Initialize the analyzer.
        
        Args:
            schema: Optional GraphQL schema for type information
        """
        super().__init__()  # Initialize parent Visitor
        self.schema = schema
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
        self.score.field_count += 1
        
        # Track field name patterns that suggest arrays
        field_name = node.name.value
        # Common plural patterns and array-like field names
        array_patterns = [
            "list", "items", "all", "many", "users", "posts", 
            "comments", "replies", "reactions", "notifications"
        ]
        # Check if field ends with 's' (plural) or matches patterns
        if field_name.endswith("s") or any(pattern in field_name.lower() for pattern in array_patterns):
            self.score.array_field_count += 1
            self.score.array_score += 10 * (self.current_depth + 1)
        
        # Add depth score - exponentially increasing with depth
        self.score.depth_score += self.current_depth ** 2
    
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
) -> ComplexityScore:
    """Analyze the complexity of a GraphQL query.
    
    Args:
        query: GraphQL query string
        schema: Optional GraphQL schema for enhanced analysis
        
    Returns:
        ComplexityScore with analysis results
    """
    analyzer = QueryComplexityAnalyzer(schema)
    return analyzer.analyze(query)


def should_cache_query(
    query: str,
    schema: GraphQLSchema | None = None,
    complexity_threshold: int = 200,
) -> tuple[bool, ComplexityScore]:
    """Determine if a query should be cached in TurboRouter.
    
    Args:
        query: GraphQL query string
        schema: Optional GraphQL schema
        complexity_threshold: Maximum complexity for caching
        
    Returns:
        Tuple of (should_cache, complexity_score)
    """
    score = analyze_query_complexity(query, schema)
    return score.should_cache(complexity_threshold), score


def calculate_cache_weight(query: str, schema: GraphQLSchema | None = None) -> float:
    """Calculate the cache weight for a query.
    
    Args:
        query: GraphQL query string
        schema: Optional GraphQL schema
        
    Returns:
        Cache weight (0.1 to 10.0)
    """
    score = analyze_query_complexity(query, schema)
    return score.cache_weight