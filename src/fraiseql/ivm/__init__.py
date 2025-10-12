"""Incremental View Maintenance (IVM) integration for FraiseQL.

This module provides automatic detection and setup of incremental maintenance
for denormalized JSONB tables (tv_ prefixed) using the jsonb_ivm PostgreSQL extension.

CQRS Architecture:
    - tb_* tables: Normalized relational data (command side)
    - tv_* tables: Denormalized JSONB projections (query side)

Instead of full rebuilds when tb_ tables change, this module enables incremental
updates using jsonb_merge_shallow() for 10-100x faster updates.

Features:
    - Automatic tv_ table complexity analysis
    - IVM candidate detection based on update patterns
    - Trigger generation for incremental tb_ → tv_ sync
    - Performance monitoring and recommendations
"""

from fraiseql.ivm.analyzer import (
    IVMAnalyzer,
    IVMCandidate,
    IVMRecommendation,
    setup_auto_ivm,
)

__all__ = [
    "IVMAnalyzer",
    "IVMCandidate",
    "IVMRecommendation",
    "setup_auto_ivm",
]
