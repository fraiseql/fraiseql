"""Common output types for Blog Demo Application.

Following PrintOptim Backend patterns for consistent mutation result structures
and common GraphQL types used across the application.
"""

import uuid
from typing import Any, Dict, List, Optional
from datetime import datetime

import fraiseql
from fraiseql import FraiseQLError


# ============================================================================
# BASE TYPES - Common output structures
# ============================================================================

@fraiseql.type
class MutationResultBase:
    """Base class for mutation results following PrintOptim patterns.

    Provides consistent structure for all mutation responses with:
    - Standard message field for user feedback
    - Original payload preservation for debugging
    - Error arrays using clean FraiseQL patterns
    """

    message: Optional[str] = None
    original_payload: Optional[Dict[str, Any]] = None


@fraiseql.type
class AuditInfo:
    """Audit information for entities."""

    created_at: datetime
    created_by: Optional[uuid.UUID] = None
    updated_at: datetime
    updated_by: Optional[uuid.UUID] = None
    version: int = 1


@fraiseql.type
class PaginationInfo:
    """Pagination metadata for list queries."""

    total_count: int
    page_size: int
    current_page: int
    total_pages: int
    has_next_page: bool
    has_previous_page: bool


@fraiseql.type
class EntityReference:
    """Reference to another entity."""

    id: uuid.UUID
    identifier: Optional[str] = None
    name: Optional[str] = None
    type: Optional[str] = None


# ============================================================================
# ERROR TYPES - Structured error information
# ============================================================================

@fraiseql.type
class ValidationError:
    """Detailed validation error information."""

    field: str
    message: str
    code: str
    value: Optional[str] = None
    constraints: Optional[Dict[str, Any]] = None


@fraiseql.type
class BusinessRuleViolation:
    """Business rule violation details."""

    rule: str
    message: str
    code: str
    context: Optional[Dict[str, Any]] = None
    suggested_actions: List[str] = []


@fraiseql.type
class ConflictInfo:
    """Information about conflicts (duplicates, constraints)."""

    conflicting_field: str
    conflicting_value: str
    existing_entity: Optional[EntityReference] = None
    message: str
    suggestions: List[str] = []


# ============================================================================
# OPERATION RESULT TYPES - Status and metadata
# ============================================================================

@fraiseql.type
class OperationMetadata:
    """Metadata about the performed operation."""

    operation_type: str  # CREATE, UPDATE, DELETE, PUBLISH, etc.
    operation_id: Optional[str] = None
    performed_at: datetime
    performed_by: Optional[uuid.UUID] = None
    duration_ms: Optional[int] = None
    affected_entities: List[EntityReference] = []


@fraiseql.type
class ChangeInfo:
    """Information about changes made to an entity."""

    changed_fields: List[str] = []
    previous_values: Optional[Dict[str, Any]] = None
    new_values: Optional[Dict[str, Any]] = None
    change_reason: Optional[str] = None


# ============================================================================
# SYSTEM STATUS TYPES - Health and monitoring
# ============================================================================

@fraiseql.type
class SystemHealth:
    """System health information."""

    status: str  # healthy, degraded, unhealthy
    message: str
    checks: Dict[str, bool]
    timestamp: datetime
    version: str


@fraiseql.type
class PerformanceMetrics:
    """Performance metrics for operations."""

    query_count: int
    mutation_count: int
    average_response_time_ms: int
    cache_hit_rate: float
    database_connections: int
    memory_usage_mb: int


# ============================================================================
# UTILITY TYPES - Common data structures
# ============================================================================

@fraiseql.type
class KeyValuePair:
    """Simple key-value pair."""

    key: str
    value: str
    description: Optional[str] = None


@fraiseql.type
class SelectOption:
    """Option for select inputs."""

    value: str
    label: str
    description: Optional[str] = None
    disabled: bool = False
    group: Optional[str] = None


@fraiseql.type
class FileInfo:
    """File information."""

    filename: str
    size: int
    mime_type: str
    url: str
    uploaded_at: datetime
    uploaded_by: Optional[uuid.UUID] = None


# ============================================================================
# SEARCH AND FILTER TYPES
# ============================================================================

@fraiseql.type
class SearchResult:
    """Search result with highlighting and relevance."""

    entity: EntityReference
    relevance_score: float
    highlights: List[str] = []
    matched_fields: List[str] = []


@fraiseql.type
class FilterOption:
    """Available filter option with counts."""

    field: str
    value: str
    label: str
    count: int
    selected: bool = False


# ============================================================================
# NOTIFICATION AND COMMUNICATION TYPES
# ============================================================================

@fraiseql.type
class NotificationInfo:
    """Notification information."""

    id: uuid.UUID
    title: str
    message: str
    type: str  # info, warning, error, success
    read: bool = False
    created_at: datetime
    expires_at: Optional[datetime] = None


@fraiseql.type
class EmailInfo:
    """Email sending information."""

    to_addresses: List[str]
    subject: str
    template: str
    variables: Optional[Dict[str, Any]] = None
    sent_at: Optional[datetime] = None
    status: str  # pending, sent, failed


# ============================================================================
# INTEGRATION TYPES - External services
# ============================================================================

@fraiseql.type
class WebhookInfo:
    """Webhook information."""

    url: str
    method: str
    headers: Optional[Dict[str, str]] = None
    payload: Optional[Dict[str, Any]] = None
    status: str
    response_code: Optional[int] = None
    response_body: Optional[str] = None
    triggered_at: datetime


@fraiseql.type
class ExternalServiceStatus:
    """External service status."""

    service_name: str
    status: str  # available, degraded, unavailable
    response_time_ms: Optional[int] = None
    last_checked_at: datetime
    error_message: Optional[str] = None
