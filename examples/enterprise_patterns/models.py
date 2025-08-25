"""Enterprise patterns models demonstrating all FraiseQL enterprise patterns."""

from datetime import datetime
from decimal import Decimal
from typing import Annotated, Any, Optional
from uuid import UUID

from pydantic import Field

import fraiseql
from fraiseql import fraise_field

# Base Audit Pattern


@fraiseql.type
class AuditTrail:
    """Complete audit trail information."""

    created_at: datetime
    created_by_id: UUID
    created_by_name: str
    updated_at: Optional[datetime] = None
    updated_by_id: Optional[UUID] = None
    updated_by_name: Optional[str] = None
    version: int
    change_reason: Optional[str] = None
    updated_fields: Optional[list[str]] = None
    source_system: str = "api"
    correlation_id: Optional[str] = None


# Core Entity Types with Enterprise Features


@fraiseql.type
class Organization:
    """Organization with complete enterprise features."""

    id: UUID  # Exposed as GraphQL ID
    name: str
    identifier: str  # Business identifier (ORG-2024-ACME)

    # Business fields
    legal_name: str
    tax_id: Optional[str] = None
    industry: Optional[str] = None
    employee_count: Optional[int] = None
    annual_revenue: Optional[Decimal] = None

    # Enterprise features
    audit_trail: AuditTrail
    is_active: bool = True
    settings: dict[str, Any] = fraise_field(default_factory=dict)


@fraiseql.type
class User:
    """User with comprehensive audit and role management."""

    id: UUID
    email: str
    name: str
    identifier: str  # USER-JOHN-SMITH-001

    # Profile information
    first_name: str
    last_name: str
    avatar_url: Optional[str] = None
    bio: Optional[str] = None
    phone: Optional[str] = None

    # Authentication and authorization
    is_active: bool = True
    is_verified: bool = False
    roles: list[str] = fraise_field(default_factory=list)
    permissions: list[str] = fraise_field(default_factory=list)

    # Enterprise features
    audit_trail: AuditTrail
    organization_id: UUID
    department: Optional[str] = None
    job_title: Optional[str] = None
    manager_id: Optional[UUID] = None

    # Usage tracking
    last_login_at: Optional[datetime] = None
    login_count: int = 0
    failed_login_attempts: int = 0

    # Preferences
    timezone: str = "UTC"
    language: str = "en"
    notification_preferences: dict[str, bool] = fraise_field(default_factory=dict)


@fraiseql.type
class Project:
    """Project entity demonstrating complex business logic."""

    id: UUID
    name: str
    identifier: str  # PROJ-2024-Q1-WEBSITE

    # Project details
    description: Optional[str] = None
    status: str  # draft, active, on_hold, completed, cancelled
    priority: str = "medium"  # low, medium, high, critical

    # Relationships
    organization_id: UUID
    owner_id: UUID
    team_member_ids: list[UUID] = fraise_field(default_factory=list)

    # Timeline
    start_date: Optional[datetime] = None
    due_date: Optional[datetime] = None
    completed_at: Optional[datetime] = None

    # Budget and tracking
    budget: Optional[Decimal] = None
    spent: Decimal = Decimal("0.00")
    estimated_hours: Optional[int] = None
    actual_hours: int = 0

    # Enterprise features
    audit_trail: AuditTrail
    tags: list[str] = fraise_field(default_factory=list)
    custom_fields: dict[str, Any] = fraise_field(default_factory=dict)

    # Calculated fields (populated by views)
    task_count: Optional[int] = None
    completed_task_count: Optional[int] = None
    progress_percentage: Optional[float] = None


@fraiseql.type
class Task:
    """Task with nested relationships and complex validation."""

    id: UUID
    title: str
    identifier: str  # TASK-PROJ-001-SETUP

    # Task details
    description: Optional[str] = None
    status: str  # TODO, in_progress, review, done, cancelled
    priority: str = "medium"

    # Relationships
    project_id: UUID
    assignee_id: Optional[UUID] = None
    reporter_id: UUID
    parent_task_id: Optional[UUID] = None

    # Timeline
    due_date: Optional[datetime] = None
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None

    # Effort tracking
    estimated_hours: Optional[float] = None
    actual_hours: float = 0.0

    # Enterprise features
    audit_trail: AuditTrail
    labels: list[str] = fraise_field(default_factory=list)

    # Calculated fields
    subtask_count: Optional[int] = None
    blocked_by_count: Optional[int] = None
    is_overdue: Optional[bool] = None


# Input Types with Enterprise Validation


@fraiseql.input
class CreateOrganizationInput:
    """Organization creation with comprehensive validation."""

    name: Annotated[str, Field(min_length=2, max_length=200)]
    legal_name: Annotated[str, Field(min_length=2, max_length=500)]
    industry: Optional[Annotated[str, Field(max_length=100)]] = None

    # Optional business information
    tax_id: Optional[Annotated[str, Field(pattern=r"^[0-9-]+$")]] = None
    employee_count: Optional[Annotated[int, Field(gt=0, le=1000000)]] = None
    annual_revenue: Optional[Annotated[Decimal, Field(gt=0)]] = None

    # Enterprise metadata
    _change_reason: Optional[str] = None
    _source_system: str = "api"


@fraiseql.input
class CreateUserInput:
    """User creation with multi-layer validation."""

    email: Annotated[str, Field(regex=r"^[^@]+@[^@]+\.[^@]+$")]
    first_name: Annotated[str, Field(min_length=1, max_length=50)]
    last_name: Annotated[str, Field(min_length=1, max_length=50)]

    # Optional profile fields
    bio: Optional[Annotated[str, Field(max_length=1000)]] = None
    phone: Optional[Annotated[str, Field(pattern=r"^\+?[1-9]\d{1,14}$")]] = None

    # Organizational assignment
    organization_id: UUID
    department: Optional[str] = None
    job_title: Optional[str] = None
    manager_id: Optional[UUID] = None

    # Initial roles and permissions
    roles: list[str] = fraise_field(default_factory=lambda: ["user"])

    # Enterprise metadata
    _change_reason: Optional[str] = None
    _send_welcome_email: bool = True


@fraiseql.input
class CreateProjectInput:
    """Project creation with business rule validation."""

    name: Annotated[str, Field(min_length=3, max_length=200)]
    description: Optional[Annotated[str, Field(max_length=2000)]] = None

    # Project setup
    organization_id: UUID
    owner_id: UUID
    status: str = "draft"
    priority: str = "medium"

    # Timeline
    start_date: Optional[datetime] = None
    due_date: Optional[datetime] = None

    # Budget
    budget: Optional[Annotated[Decimal, Field(gt=0)]] = None
    estimated_hours: Optional[Annotated[int, Field(gt=0, le=10000)]] = None

    # Team assignment
    team_member_ids: list[UUID] = fraise_field(default_factory=list)
    tags: list[str] = fraise_field(default_factory=list)

    # Enterprise metadata
    _change_reason: Optional[str] = None
    _template_id: Optional[UUID] = None  # For project templates


@fraiseql.input
class CreateTaskInput:
    """Task creation with complex validation."""

    title: Annotated[str, Field(min_length=3, max_length=200)]
    description: Optional[Annotated[str, Field(max_length=2000)]] = None

    # Task assignment
    project_id: UUID
    assignee_id: Optional[UUID] = None
    parent_task_id: Optional[UUID] = None

    # Planning
    priority: str = "medium"
    due_date: Optional[datetime] = None
    estimated_hours: Optional[Annotated[float, Field(gt=0, le=1000)]] = None

    # Categorization
    labels: list[str] = fraise_field(default_factory=list)

    # Enterprise metadata
    _change_reason: Optional[str] = None
    _copy_from_task_id: Optional[UUID] = None


# Success Types with Rich Metadata


@fraiseql.success
class CreateOrganizationSuccess:
    """Organization created successfully with audit information."""

    organization: Organization
    message: str = "Organization created successfully"

    # Enterprise metadata
    generated_identifier: str
    initial_setup_completed: bool = False
    welcome_email_sent: bool = False
    audit_metadata: dict[str, Any]


@fraiseql.success
class CreateUserSuccess:
    """User created successfully with onboarding info."""

    user: User
    message: str = "User created successfully"

    # Enterprise features
    generated_identifier: str
    initial_password_set: bool = False
    welcome_email_queued: bool = False
    role_assignments: list[dict[str, str]]
    audit_metadata: dict[str, Any]


@fraiseql.success
class CreateProjectSuccess:
    """Project created with setup information."""

    project: Project
    message: str = "Project created successfully"

    # Project setup results
    generated_identifier: str
    team_notifications_sent: int = 0
    template_applied: Optional[str] = None
    initial_tasks_created: int = 0
    audit_metadata: dict[str, Any]


@fraiseql.success
class CreateTaskSuccess:
    """Task created with relationship validation."""

    task: Task
    message: str = "Task created successfully"

    # Task creation results
    generated_identifier: str
    assignee_notified: bool = False
    parent_task_updated: bool = False
    project_stats_updated: bool = False
    audit_metadata: dict[str, Any]


# NOOP Types for Business Rule Handling


@fraiseql.success
class CreateOrganizationNoop:
    """Organization creation was a no-op."""

    existing_organization: Organization
    message: str
    noop_reason: str

    # NOOP context
    conflict_field: str  # name, legal_name, tax_id
    attempted_value: str
    business_rule_violated: Optional[str] = None
    suggested_action: Optional[str] = None


@fraiseql.success
class CreateUserNoop:
    """User creation was a no-op."""

    existing_user: User
    message: str
    noop_reason: str

    # User-specific NOOP context
    conflict_field: str  # email, identifier
    attempted_email: Optional[str] = None
    organization_mismatch: bool = False
    invitation_already_sent: bool = False


@fraiseql.success
class CreateProjectNoop:
    """Project creation was a no-op."""

    existing_project: Project
    message: str
    noop_reason: str

    # Project-specific NOOP context
    name_conflict_in_organization: bool = False
    owner_permission_insufficient: bool = False
    budget_exceeds_organization_limit: bool = False
    template_unavailable: bool = False


@fraiseql.success
class CreateTaskNoop:
    """Task creation was a no-op."""

    existing_task: Optional[Task] = None
    message: str
    noop_reason: str

    # Task-specific NOOP context
    project_not_accepting_tasks: bool = False
    parent_task_completed: bool = False
    assignee_unavailable: bool = False
    duplicate_title_in_project: bool = False


# Error Types with Detailed Context


@fraiseql.failure
class CreateOrganizationError:
    """Organization creation failed with context."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None

    # Enterprise error context
    validation_failures: list[dict[str, str]]
    business_rule_violations: list[str]
    system_constraints: list[str]
    suggested_fixes: list[str]


@fraiseql.failure
class CreateUserError:
    """User creation failed with detailed information."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None

    # User-specific error context
    email_validation_failed: bool = False
    organization_capacity_exceeded: bool = False
    role_assignment_failed: list[str] = fraise_field(default_factory=list)
    invitation_delivery_failed: bool = False

    # Compliance context
    data_privacy_violations: list[str] = fraise_field(default_factory=list)
    security_policy_violations: list[str] = fraise_field(default_factory=list)


@fraiseql.failure
class CreateProjectError:
    """Project creation failed with business context."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None

    # Project-specific errors
    budget_validation_failed: bool = False
    timeline_validation_failed: bool = False
    team_assignment_failed: list[UUID] = fraise_field(default_factory=list)
    template_application_failed: bool = False

    # Resource constraints
    organization_project_limit_exceeded: bool = False
    insufficient_permissions: list[str] = fraise_field(default_factory=list)


@fraiseql.failure
class CreateTaskError:
    """Task creation failed with relationship context."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None

    # Task-specific errors
    project_validation_failed: bool = False
    assignee_validation_failed: bool = False
    parent_task_validation_failed: bool = False
    timeline_conflict: bool = False

    # Capacity constraints
    assignee_workload_exceeded: bool = False
    project_task_limit_exceeded: bool = False


# Update Input Types (showing enterprise patterns for updates)


@fraiseql.input
class UpdateProjectInput:
    """Project update with optimistic locking."""

    name: Optional[Annotated[str, Field(min_length=3, max_length=200)]] = None
    description: Optional[Annotated[str, Field(max_length=2000)]] = None
    status: Optional[str] = None
    priority: Optional[str] = None

    # Timeline updates
    start_date: Optional[datetime] = None
    due_date: Optional[datetime] = None

    # Budget updates
    budget: Optional[Annotated[Decimal, Field(gt=0)]] = None

    # Team updates
    add_team_members: list[UUID] = fraise_field(default_factory=list)
    remove_team_members: list[UUID] = fraise_field(default_factory=list)

    # Enterprise features
    _expected_version: Optional[int] = None  # Optimistic locking
    _change_reason: Optional[str] = None
    _notify_team: bool = True


@fraiseql.success
class UpdateProjectSuccess:
    """Project updated with change tracking."""

    project: Project
    message: str = "Project updated successfully"

    # Change tracking
    updated_fields: list[str]
    previous_version: int
    new_version: int

    # Business impact
    timeline_changed: bool = False
    budget_changed: bool = False
    team_changed: bool = False
    status_changed: bool = False

    # Notifications
    team_members_notified: int = 0
    stakeholders_notified: int = 0

    audit_metadata: dict[str, Any]


@fraiseql.success
class UpdateProjectNoop:
    """Project update was a no-op."""

    project: Project
    message: str = "No changes detected"
    noop_reason: str = "no_changes"

    # NOOP context
    fields_checked: list[str]
    version_conflict: bool = False
    permission_denied_fields: list[str] = fraise_field(default_factory=list)
    business_rule_prevented_changes: list[str] = fraise_field(default_factory=list)


@fraiseql.failure
class UpdateProjectError:
    """Project update failed with context."""

    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None

    # Update-specific errors
    version_conflict: bool = False
    concurrent_modification_detected: bool = False
    status_transition_invalid: bool = False
    timeline_validation_failed: bool = False

    # Current state context
    current_version: Optional[int] = None
    expected_version: Optional[int] = None
    last_modified_by: Optional[str] = None
    last_modified_at: Optional[datetime] = None
