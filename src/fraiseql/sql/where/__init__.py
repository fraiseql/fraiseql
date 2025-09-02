"""FraiseQL Where Functionality - Clean Marie Kondo Architecture.

This module provides a clean, composable approach to building SQL WHERE clauses
from GraphQL filters. It replaces the complex strategy pattern with simple,
testable functions organized by concern.

Key principles:
- Simple functions over complex classes
- Clear separation of concerns
- Easy to test in isolation
- Foundation for logical operators (AND/OR/NOT)
"""

from .core.field_detection import FieldType, detect_field_type
from .core.sql_builder import build_where_clause
from .operators import get_operator_function

__all__ = [
    "FieldType",
    "build_where_clause",
    "detect_field_type",
    "get_operator_function",
]
