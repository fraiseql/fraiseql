"""Operator strategies for WHERE clause SQL generation.

Public API for all operator strategies. This module maintains backward
compatibility with the old `operator_strategies.py` module.
"""

from .advanced import CoordinateOperatorStrategy, JsonbOperatorStrategy

# Import advanced strategies (Phase 4)
from .array import ArrayOperatorStrategy
from .base import BaseOperatorStrategy, OperatorStrategyError
from .core import BooleanOperatorStrategy, NumericOperatorStrategy, StringOperatorStrategy

# Import fallback strategies (Phase 4)
from .fallback import (
    ComparisonOperatorStrategy,
    ListOperatorStrategy,
    NullOperatorStrategy,
    PathOperatorStrategy,
    PatternOperatorStrategy,
)
from .postgresql import (
    DateRangeOperatorStrategy,
    LTreeOperatorStrategy,
    MacAddressOperatorStrategy,
    NetworkOperatorStrategy,
)
from .strategy_registry import OperatorRegistry, get_default_registry, register_operator

# Auto-register core strategies (Phase 2)
register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
register_operator(BooleanOperatorStrategy())

# Auto-register PostgreSQL-specific strategies (Phase 3)
register_operator(NetworkOperatorStrategy())
register_operator(LTreeOperatorStrategy())
register_operator(DateRangeOperatorStrategy())
register_operator(MacAddressOperatorStrategy())

# Auto-register advanced strategies (Phase 4)
# CRITICAL: ArrayOperatorStrategy must come BEFORE ComparisonOperatorStrategy
# to handle array-specific operations properly
register_operator(NullOperatorStrategy())  # Always handle isnull first
register_operator(ArrayOperatorStrategy())  # Handle array ops before generic comparison
register_operator(CoordinateOperatorStrategy())  # Handle coordinate ops before generic comparison
register_operator(JsonbOperatorStrategy())  # JSONB-specific operators

# Register fallback strategies LAST (Phase 4)
# These catch any operators not handled by more specific strategies
register_operator(ComparisonOperatorStrategy())
register_operator(PatternOperatorStrategy())
register_operator(ListOperatorStrategy())
register_operator(PathOperatorStrategy())

# Re-export for backward compatibility
__all__ = [
    # Advanced (Phase 4)
    "ArrayOperatorStrategy",
    "BaseOperatorStrategy",
    "BooleanOperatorStrategy",
    # Fallback (Phase 4)
    "ComparisonOperatorStrategy",
    "CoordinateOperatorStrategy",
    "DateRangeOperatorStrategy",
    "JsonbOperatorStrategy",
    "LTreeOperatorStrategy",
    "ListOperatorStrategy",
    "MacAddressOperatorStrategy",
    "NetworkOperatorStrategy",
    "NullOperatorStrategy",
    "NumericOperatorStrategy",
    "OperatorRegistry",
    "OperatorStrategyError",
    "PathOperatorStrategy",
    "PatternOperatorStrategy",
    "StringOperatorStrategy",
    "get_default_registry",
    "register_operator",
]
