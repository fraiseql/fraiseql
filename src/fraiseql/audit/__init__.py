"""Security audit and event logging for FraiseQL.

This package provides:
- SecurityLogger: Centralized security event logging (Phase 14)
- AuditLogQueryBuilder: Query builder for audit logs (Commit 5)
- AuditAnalyzer: Analysis helpers for audit events (Commit 5)
"""

from .analyzer import AuditAnalyzer
from .models import (
    AuditEvent,
    AuditFilterType,
    ComplianceReport,
    EventStats,
    OperationType,
)
from .query_builder import AuditLogQueryBuilder
from .security_logger import (
    SecurityEvent,
    SecurityEventSeverity,
    SecurityEventType,
    SecurityLogger,
    get_security_logger,
    set_security_logger,
)

__all__ = [
    "AuditAnalyzer",
    "AuditEvent",
    "AuditFilterType",
    "AuditLogQueryBuilder",
    "ComplianceReport",
    "EventStats",
    "OperationType",
    "SecurityEvent",
    "SecurityEventSeverity",
    "SecurityEventType",
    "SecurityLogger",
    "get_security_logger",
    "set_security_logger",
]
