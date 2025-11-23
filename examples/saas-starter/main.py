#!/usr/bin/env python
"""SaaS Starter Template - Main Application.

Multi-tenant SaaS application with organization management, team invitations,
subscription billing, and usage tracking.
"""

import os
from contextlib import asynccontextmanager
from uuid import UUID

import uvicorn
from fastapi import FastAPI, Request

# Import models
from models import (
    ActivityLogEntry,
    Organization,
    OrganizationUpdateInput,
    Project,
    ProjectCreateInput,
    ProjectUpdateInput,
    RegisterInput,
    Subscription,
    TeamInvitation,
    UsageLimits,
    UsageMetrics,
    User,
)

import fraiseql
from fraiseql import Info
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

# Configuration
DATABASE_URL = os.getenv("DATABASE_URL", "postgresql://localhost/saas_starter")
JWT_SECRET = os.getenv("JWT_SECRET", "change-this-secret-in-production")
CORS_ORIGINS = os.getenv("CORS_ORIGINS", "http://localhost:3000").split(",")


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager."""
    print("=" * 70)
    print("SaaS Starter API Starting...")
    print("=" * 70)
    yield
    print("\nSaaS Starter API Shutting Down...")


# Configure FraiseQL
config = FraiseQLConfig(
    database_url=DATABASE_URL,
    enable_playground=True,
    cors_origins=CORS_ORIGINS,
    pool_size=20,
    max_overflow=10,
)

# Create FraiseQL FastAPI app
app = create_fraiseql_app(
    config=config,
    title="SaaS Starter API",
    version="1.0.0",
    description="Multi-tenant SaaS starter template with FraiseQL",
    lifespan=lifespan,
)

# Register GraphQL types
app.register_type(Organization)
app.register_type(User)
app.register_type(Subscription)
app.register_type(UsageMetrics)
app.register_type(UsageLimits)
app.register_type(TeamInvitation)
app.register_type(ActivityLogEntry)
app.register_type(Project)

# Register input types
app.register_input_type(RegisterInput)
app.register_input_type(OrganizationUpdateInput)
app.register_input_type(ProjectCreateInput)
app.register_input_type(ProjectUpdateInput)


# ============================================================================
# QUERIES
# ============================================================================


@fraiseql.query
async def current_organization(info: Info) -> Organization:
    """Get current user's organization."""
    org_id = info.context["organization_id"]
    db = info.context["db"]
    return await db.find_one("v_organization", "organization", info, id=org_id)


@fraiseql.query
async def current_user(info: Info) -> User:
    """Get current authenticated user."""
    user_id = info.context["user_id"]
    db = info.context["db"]
    return await db.find_one("v_user", "user", info, id=user_id)


@fraiseql.query
async def team_members(info: Info) -> list[User]:
    """Get all team members in current organization."""
    org_id = info.context["organization_id"]
    db = info.context["db"]
    return await db.find("v_user", "users", info, fk_organization=org_id)


@fraiseql.query
async def projects(info: Info, limit: int = 50) -> list[Project]:
    """Get projects for current organization."""
    org_id = info.context["organization_id"]
    db = info.context["db"]
    return await db.find("v_project", "projects", info, fk_organization=org_id, limit=limit)


@fraiseql.query
async def project(info: Info, project_id: UUID) -> Project:
    """Get project by ID (tenant-aware)."""
    org_id = info.context["organization_id"]
    db = info.context["db"]

    project = await db.find_one("v_project", "project", info, id=project_id)

    # Verify tenant isolation
    if project.organization_id != org_id:
        raise PermissionError("Access denied")

    return project


@fraiseql.query
async def usage_metrics(info: Info) -> UsageMetrics:
    """Get current billing period usage metrics."""
    from datetime import datetime

    org_id = info.context["organization_id"]
    db = info.context["db"]
    period_start = datetime.now().replace(day=1, hour=0, minute=0, second=0)

    metrics = await db.find_one(
        "v_usage_metrics", "metrics", info, organization_id=org_id, period_start=period_start
    )

    if not metrics:
        # Return default metrics
        return UsageMetrics(
            organization_id=org_id,
            period_start=period_start,
            period_end=period_start.replace(month=period_start.month + 1),
            projects=0,
            storage=0,
            api_calls=0,
            seats=1,
        )

    return metrics


@fraiseql.query
async def activity_log(info: Info, limit: int = 50) -> list[ActivityLogEntry]:
    """Get activity log for current organization."""
    org_id = info.context["organization_id"]
    db = info.context["db"]

    return await db.find(
        "v_activity_log",
        "entries",
        info,
        organization_id=org_id,
        limit=limit,
        order_by="-created_at",
    )


# ============================================================================
# MUTATIONS
# ============================================================================


@fraiseql.mutation
async def create_project(info: Info, input: ProjectCreateInput) -> Project:
    """Create new project in current organization."""
    from datetime import datetime
    from uuid import uuid4

    org_id = info.context["organization_id"]
    user_id = info.context["user_id"]

    project_id = uuid4()

    project = await info.context.repo.create(
        "projects",
        {
            "id": project_id,
            "organization_id": org_id,
            "name": input.name,
            "description": input.description,
            "owner_id": user_id,
            "status": "active",
            "settings": {},
        },
    )

    # Track usage
    await info.context.repo.execute("SELECT track_usage(%s, 'projects', 1)", [org_id])

    # Log activity
    await info.context.repo.create(
        "activity_log",
        {
            "organization_id": org_id,
            "user_id": user_id,
            "action": "create_project",
            "resource": "project",
            "resource_id": project_id,
            "details": {"name": input.name},
        },
    )

    return await project(info, project_id)


@fraiseql.mutation
async def update_project(info: Info, project_id: UUID, input: ProjectUpdateInput) -> Project:
    """Update project (tenant-aware)."""
    org_id = info.context["organization_id"]
    user_id = info.context["user_id"]

    # Verify ownership
    existing = await info.context.repo.find_one("projects", project_id)
    if existing["organization_id"] != org_id:
        raise PermissionError("Access denied")

    # Build update data
    update_data = {}
    if input.name:
        update_data["name"] = input.name
    if input.description is not None:
        update_data["description"] = input.description
    if input.status:
        update_data["status"] = input.status

    # Update project
    await info.context.repo.update("projects", project_id, update_data)

    # Log activity
    await info.context.repo.create(
        "activity_log",
        {
            "organization_id": org_id,
            "user_id": user_id,
            "action": "update_project",
            "resource": "project",
            "resource_id": project_id,
            "details": update_data,
        },
    )

    return await project(info, project_id)


@fraiseql.mutation
async def delete_project(info: Info, project_id: UUID) -> bool:
    """Delete project (tenant-aware)."""
    org_id = info.context["organization_id"]
    user_id = info.context["user_id"]

    # Verify ownership
    existing = await info.context.repo.find_one("projects", project_id)
    if existing["organization_id"] != org_id:
        raise PermissionError("Access denied")

    # Delete project
    await info.context.repo.delete("projects", project_id)

    # Log activity
    await info.context.repo.create(
        "activity_log",
        {
            "organization_id": org_id,
            "user_id": user_id,
            "action": "delete_project",
            "resource": "project",
            "resource_id": project_id,
            "details": {"name": existing["name"]},
        },
    )

    return True


@fraiseql.mutation
async def update_organization(info: Info, input: OrganizationUpdateInput) -> Organization:
    """Update organization settings."""
    org_id = info.context["organization_id"]
    user_id = info.context["user_id"]

    # Build update data
    update_data = {}
    if input.name:
        update_data["name"] = input.name
    if input.settings:
        update_data["settings"] = input.settings

    # Update organization
    await info.context.repo.update("organizations", org_id, update_data)

    # Log activity
    await info.context.repo.create(
        "activity_log",
        {
            "organization_id": org_id,
            "user_id": user_id,
            "action": "update_organization",
            "resource": "organization",
            "resource_id": org_id,
            "details": update_data,
        },
    )

    return await current_organization(info)


# Register queries
app.register_query(current_organization)
app.register_query(current_user)
app.register_query(team_members)
app.register_query(projects)
app.register_query(project)
app.register_query(usage_metrics)
app.register_query(activity_log)

# Register mutations
app.register_mutation(create_project)
app.register_mutation(update_project)
app.register_mutation(delete_project)
app.register_mutation(update_organization)


# ============================================================================
# FASTAPI ROUTES
# ============================================================================


@app.get("/")
async def root():
    """API information."""
    return {
        "name": "SaaS Starter API",
        "version": "1.0.0",
        "graphql": "/graphql",
        "playground": "/graphql",
        "docs": "/docs",
        "features": [
            "Multi-tenant architecture",
            "Organization management",
            "Team invitations",
            "Subscription billing",
            "Usage tracking",
            "Activity logs",
        ],
    }


@app.get("/health")
async def health():
    """Health check."""
    return {"status": "healthy", "service": "saas-starter"}


# ============================================================================
# AUTHENTICATION MIDDLEWARE
# ============================================================================


@app.middleware("http")
async def add_tenant_context(request: Request, call_next):
    """Extract tenant context from JWT and set PostgreSQL session variable.

    In production:
    1. Decode JWT from Authorization header
    2. Extract organization_id from token payload
    3. Set PostgreSQL session variable for RLS
    4. Add user context to request state
    """
    # For demo: Use header-based tenant selection
    tenant_id = request.headers.get("X-Organization-ID", "11111111-1111-1111-1111-111111111111")
    user_id = request.headers.get("X-User-ID", "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")

    # In production: Decode JWT
    # token = request.headers.get("Authorization", "").replace("Bearer ", "")
    # payload = jwt.decode(token, JWT_SECRET, algorithms=["HS256"])
    # tenant_id = payload["organization_id"]
    # user_id = payload["user_id"]

    # Set tenant context
    request.state.user = {
        "user_id": user_id,
        "organization_id": tenant_id,
    }

    # Set PostgreSQL session variable for RLS
    # In production: Set this via connection pool
    # await db.execute("SET LOCAL app.current_tenant = %s", [tenant_id])

    response = await call_next(request)
    return response


if __name__ == "__main__":
    print("=" * 70)
    print("FraiseQL SaaS Starter Template")
    print("=" * 70)
    print()
    print("🚀 Features:")
    print("  ✅ Multi-tenant architecture with PostgreSQL RLS")
    print("  ✅ Organization & team management")
    print("  ✅ Subscription & billing integration")
    print("  ✅ Usage tracking & limits")
    print("  ✅ Activity logs & audit trail")
    print()
    print("📍 Endpoints:")
    print("  • GraphQL API:        http://localhost:8000/graphql")
    print("  • GraphQL Playground: http://localhost:8000/graphql")
    print("  • API Docs:           http://localhost:8000/docs")
    print("  • Health Check:       http://localhost:8000/health")
    print()
    print("💡 Example Query:")
    print()
    print("  query GetProjects {")
    print("    currentOrganization {")
    print("      id")
    print("      name")
    print("      plan")
    print("      memberCount")
    print("    }")
    print("    projects {")
    print("      id")
    print("      name")
    print("      description")
    print("      status")
    print("    }")
    print("  }")
    print()
    print("=" * 70)
    print()

    uvicorn.run(app, host="0.0.0.0", port=8000, log_level="info")
