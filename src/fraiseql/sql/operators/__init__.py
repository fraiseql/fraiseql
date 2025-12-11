"""Operator strategies for WHERE clause SQL generation.

Public API for all operator strategies. This module maintains backward
compatibility with the old `operator_strategies.py` module.
"""

from .base import BaseOperatorStrategy, OperatorStrategyError
from .strategy_registry import OperatorRegistry, get_default_registry, register_operator

# Re-export for backward compatibility
__all__ = [
    "BaseOperatorStrategy",
    "OperatorRegistry",
    "OperatorStrategyError",
    "get_default_registry",
    "register_operator",
]
