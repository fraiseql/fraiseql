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

# Register fallback strategies FIRST (Phase 4)
# These catch any operators not handled by more specific strategies
# IMPORTANT: Fallback strategies are checked LAST because registry uses reverse order
register_operator(PathOperatorStrategy())
register_operator(ListOperatorStrategy())
register_operator(PatternOperatorStrategy())
register_operator(ComparisonOperatorStrategy())

# Auto-register advanced strategies (Phase 4)
register_operator(JsonbOperatorStrategy())  # JSONB-specific operators
register_operator(CoordinateOperatorStrategy())  # Handle coordinate ops before generic comparison
register_operator(ArrayOperatorStrategy())  # Handle array ops before generic comparison
register_operator(NullOperatorStrategy())  # Always handle isnull first

# Auto-register PostgreSQL-specific strategies (Phase 3)
# These should be checked before fallback strategies
register_operator(MacAddressOperatorStrategy())
register_operator(DateRangeOperatorStrategy())
register_operator(LTreeOperatorStrategy())
register_operator(NetworkOperatorStrategy())

# Auto-register core strategies (Phase 2)
# These are most specific and should be checked first
register_operator(BooleanOperatorStrategy())
register_operator(NumericOperatorStrategy())
register_operator(StringOperatorStrategy())

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
