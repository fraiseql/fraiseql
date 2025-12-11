"""Operator strategies for WHERE clause SQL generation.

Public API for all operator strategies. This module maintains backward
compatibility with the old `operator_strategies.py` module.
"""

from .base import BaseOperatorStrategy, OperatorStrategyError
from .core import BooleanOperatorStrategy, NumericOperatorStrategy, StringOperatorStrategy
from .strategy_registry import OperatorRegistry, get_default_registry, register_operator

# Auto-register core strategies
register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
register_operator(BooleanOperatorStrategy())

# Re-export for backward compatibility
__all__ = [
    "BaseOperatorStrategy",
    "BooleanOperatorStrategy",
    "NumericOperatorStrategy",
    "OperatorRegistry",
    "OperatorStrategyError",
    "StringOperatorStrategy",
    "get_default_registry",
    "register_operator",
]
