"""Operator strategies for WHERE clause SQL generation.

Public API for all operator strategies. This module maintains backward
compatibility with the old `operator_strategies.py` module.
"""

from .base import BaseOperatorStrategy, OperatorStrategyError
from .core import BooleanOperatorStrategy, NumericOperatorStrategy, StringOperatorStrategy
from .postgresql import (
    DateRangeOperatorStrategy,
    LTreeOperatorStrategy,
    MacAddressOperatorStrategy,
    NetworkOperatorStrategy,
)
from .strategy_registry import OperatorRegistry, get_default_registry, register_operator

# Auto-register core strategies
register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
register_operator(BooleanOperatorStrategy())

# Auto-register PostgreSQL-specific strategies
register_operator(NetworkOperatorStrategy())
register_operator(LTreeOperatorStrategy())
register_operator(DateRangeOperatorStrategy())
register_operator(MacAddressOperatorStrategy())

# Re-export for backward compatibility
__all__ = [
    "BaseOperatorStrategy",
    "BooleanOperatorStrategy",
    "DateRangeOperatorStrategy",
    "LTreeOperatorStrategy",
    "MacAddressOperatorStrategy",
    "NetworkOperatorStrategy",
    "NumericOperatorStrategy",
    "OperatorRegistry",
    "OperatorStrategyError",
    "StringOperatorStrategy",
    "get_default_registry",
    "register_operator",
]
