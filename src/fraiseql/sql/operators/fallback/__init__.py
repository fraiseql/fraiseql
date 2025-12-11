"""Fallback operator strategies for generic operations."""

from .comparison_operators import ComparisonOperatorStrategy
from .list_operators import ListOperatorStrategy
from .null_operators import NullOperatorStrategy
from .path_operators import PathOperatorStrategy
from .pattern_operators import PatternOperatorStrategy

__all__ = [
    "ComparisonOperatorStrategy",
    "ListOperatorStrategy",
    "NullOperatorStrategy",
    "PathOperatorStrategy",
    "PatternOperatorStrategy",
]
