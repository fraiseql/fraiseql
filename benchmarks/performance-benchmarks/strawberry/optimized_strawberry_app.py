"""
Ultra-optimized Strawberry GraphQL implementation showcasing best practices.

This implementation demonstrates Strawberry's capabilities under optimal conditions:
- DataLoader for N+1 query elimination
- Connection pooling
- Query optimization
- Caching strategies
- Efficient resolvers
"""

import json
import os
import time
from collections import defaultdict
from datetime import date, datetime
from typing import Any, Optional

import asyncpg
import redis.asyncio as redis
from fastapi import FastAPI

import strawberry
from strawberry.fastapi import GraphQLRouter

# Database configuration
DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://benchmark:benchmark@localhost:5432/benchmark_db"
)

# Global connection pool and cache
connection_pool: Optional[asyncpg.Pool] = None
redis_client: Optional[redis.Redis] = None


# Performance monitoring
class PerformanceMonitor:
    def __init__(self):
        self.query_count = 0
        self.resolver_calls = defaultdict(int)
        self.dataloader_efficiency = defaultdict(lambda: {"calls": 0, "queries": 0})
        self.cache_hits = 0
        self.cache_misses = 0

    def record_query(self):
        self.query_count += 1

    def record_resolver(self, resolver_name: str):
        self.resolver_calls[resolver_name] += 1

    def record_dataloader(self, loader_name: str, batch_size: int, query_count: int = 1):
        self.dataloader_efficiency[loader_name]["calls"] += batch_size
        self.dataloader_efficiency[loader_name]["queries"] += query_count

    def record_cache_hit(self):
        self.cache_hits += 1

    def record_cache_miss(self):
        self.cache_misses += 1

    def get_stats(self):
        return {
            "total_queries": self.query_count,
            "resolver_calls": dict(self.resolver_calls),
            "dataloader_efficiency": dict(self.dataloader_efficiency),
            "cache_hit_rate": (self.cache_hits / max(1, self.cache_hits + self.cache_misses)) * 100,
        }


monitor = PerformanceMonitor()

# DataLoader implementations for N+1 elimination
import contextlib

from strawberry.dataloader import DataLoader


async def get_connection_pool():
    """Get or create the connection pool."""
    global connection_pool
    if connection_pool is None:
        connection_pool = await asyncpg.create_pool(
            DATABASE_URL,
            min_size=10,
            max_size=50,
            max_queries=10000,
            max_inactive_connection_lifetime=300,
            command_timeout=30,
        )
    return connection_pool


async def get_redis():
    """Get or create Redis client."""
    global redis_client
    if redis_client is None:
        try:
            redis_client = redis.Redis(
                host=os.environ.get("REDIS_HOST", "localhost"),
                port=int(os.environ.get("REDIS_PORT", "6379")),
                decode_responses=True,
            )
            await redis_client.ping()
        except Exception as e:
            print(f"Redis connection failed: {e}")
            redis_client = None
    return redis_client


# Cache decorator for expensive operations
def cached_resolver(ttl: int = 300):
    def decorator(func):
        async def wrapper(*args, **kwargs):
            redis_conn = await get_redis()
            if redis_conn:
                cache_key = f"strawberry:{func.__name__}:{hash(str(args) + str(kwargs))}"
                try:
                    cached_result = await redis_conn.get(cache_key)
                    if cached_result:
                        monitor.record_cache_hit()
                        return json.loads(cached_result)
                except Exception:
                    pass

            monitor.record_cache_miss()
            result = await func(*args, **kwargs)

            if redis_conn:
                with contextlib.suppress(Exception):
                    await redis_conn.setex(cache_key, ttl, json.dumps(result, default=str))

            return result

        return wrapper

    return decorator


# DataLoader for departments by organization
async def load_departments_by_organization(organization_ids: list[str]) -> list[list[dict]]:
    """Efficiently load departments for multiple organizations."""
    pool = await get_connection_pool()
    monitor.record_dataloader("departments_by_org", len(organization_ids), 1)

    async with pool.acquire() as conn:
        query = """
        SELECT organization_id,
               array_agg(
                   json_build_object(
                       'id', id::text,
                       'name', name,
                       'code', code,
                       'budget', budget,
                       'head_count', head_count,
                       'created_at', created_at,
                       'updated_at', updated_at
                   )
               ) as departments
        FROM benchmark.departments
        WHERE organization_id = ANY($1)
        GROUP BY organization_id
        """
        rows = await conn.fetch(query, organization_ids)

        # Create a mapping of org_id -> departments
        dept_map = {str(row["organization_id"]): row["departments"] for row in rows}

        # Return departments in the same order as requested organization_ids
        return [dept_map.get(org_id, []) for org_id in organization_ids]


# DataLoader for teams by department
async def load_teams_by_department(department_ids: list[str]) -> list[list[dict]]:
    """Efficiently load teams for multiple departments."""
    pool = await get_connection_pool()
    monitor.record_dataloader("teams_by_dept", len(department_ids), 1)

    async with pool.acquire() as conn:
        query = """
        SELECT department_id,
               array_agg(
                   json_build_object(
                       'id', id::text,
                       'name', name,
                       'description', description,
                       'formation_date', formation_date,
                       'is_active', is_active,
                       'performance_metrics', performance_metrics
                   )
               ) as teams
        FROM benchmark.teams
        WHERE department_id = ANY($1) AND is_active = true
        GROUP BY department_id
        """
        rows = await conn.fetch(query, department_ids)

        team_map = {str(row["department_id"]): row["teams"] for row in rows}
        return [team_map.get(dept_id, []) for dept_id in department_ids]


# DataLoader for employees by team
async def load_employees_by_team(team_ids: list[str]) -> list[list[dict]]:
    """Efficiently load employees for multiple teams."""
    pool = await get_connection_pool()
    monitor.record_dataloader("employees_by_team", len(team_ids), 1)

    async with pool.acquire() as conn:
        query = """
        SELECT team_id,
               array_agg(
                   json_build_object(
                       'id', id::text,
                       'email', email,
                       'username', username,
                       'full_name', full_name,
                       'role', role,
                       'level', level,
                       'salary', salary,
                       'hire_date', hire_date,
                       'skills', skills,
                       'certifications', certifications,
                       'created_at', created_at
                   ) ORDER BY level DESC, full_name
               ) as employees
        FROM benchmark.employees
        WHERE team_id = ANY($1)
        GROUP BY team_id
        """
        rows = await conn.fetch(query, team_ids)

        emp_map = {str(row["team_id"]): row["employees"] for row in rows}
        return [emp_map.get(team_id, []) for team_id in team_ids]


# DataLoader for projects by department
async def load_projects_by_department(department_ids: list[str]) -> list[list[dict]]:
    """Efficiently load projects for multiple departments."""
    pool = await get_connection_pool()
    monitor.record_dataloader("projects_by_dept", len(department_ids), 1)

    async with pool.acquire() as conn:
        query = """
        SELECT p.department_id,
               array_agg(
                   json_build_object(
                       'id', p.id::text,
                       'name', p.name,
                       'description', p.description,
                       'status', p.status,
                       'priority', p.priority,
                       'budget', p.budget,
                       'start_date', p.start_date,
                       'end_date', p.end_date,
                       'milestones', p.milestones,
                       'dependencies', p.dependencies,
                       'lead_employee_id', p.lead_employee_id::text,
                       'task_count', task_counts.count,
                       'team_size', member_counts.count
                   ) ORDER BY p.priority DESC, p.created_at DESC
               ) as projects
        FROM benchmark.projects p
        LEFT JOIN LATERAL (
            SELECT COUNT(*) as count
            FROM benchmark.tasks t
            WHERE t.project_id = p.id
        ) task_counts ON true
        LEFT JOIN LATERAL (
            SELECT COUNT(*) as count
            FROM benchmark.project_members pm
            WHERE pm.project_id = p.id
        ) member_counts ON true
        WHERE p.department_id = ANY($1)
        GROUP BY p.department_id
        """
        rows = await conn.fetch(query, department_ids)

        proj_map = {str(row["department_id"]): row["projects"] for row in rows}
        return [proj_map.get(dept_id, []) for dept_id in department_ids]


# DataLoader for project members
async def load_project_members(project_ids: list[str]) -> list[list[dict]]:
    """Efficiently load project members."""
    pool = await get_connection_pool()
    monitor.record_dataloader("project_members", len(project_ids), 1)

    async with pool.acquire() as conn:
        query = """
        SELECT pm.project_id,
               array_agg(
                   json_build_object(
                       'id', e.id::text,
                       'full_name', e.full_name,
                       'email', e.email,
                       'role', pm.role,
                       'allocation_percentage', pm.allocation_percentage,
                       'start_date', pm.start_date,
                       'end_date', pm.end_date
                   ) ORDER BY pm.allocation_percentage DESC
               ) as members
        FROM benchmark.project_members pm
        JOIN benchmark.employees e ON pm.employee_id = e.id
        WHERE pm.project_id = ANY($1)
        GROUP BY pm.project_id
        """
        rows = await conn.fetch(query, project_ids)

        member_map = {str(row["project_id"]): row["members"] for row in rows}
        return [member_map.get(proj_id, []) for proj_id in project_ids]


# DataLoader for tasks by project
async def load_tasks_by_project(project_ids: list[str]) -> list[list[dict]]:
    """Efficiently load tasks for multiple projects."""
    pool = await get_connection_pool()
    monitor.record_dataloader("tasks_by_project", len(project_ids), 1)

    async with pool.acquire() as conn:
        query = """
        SELECT t.project_id,
               array_agg(
                   json_build_object(
                       'id', t.id::text,
                       'title', t.title,
                       'description', t.description,
                       'status', t.status,
                       'priority', t.priority,
                       'estimated_hours', t.estimated_hours,
                       'actual_hours', t.actual_hours,
                       'due_date', t.due_date,
                       'tags', t.tags,
                       'assigned_to', CASE
                           WHEN e.id IS NOT NULL THEN json_build_object(
                               'id', e.id::text,
                               'full_name', e.full_name,
                               'email', e.email
                           )
                           ELSE NULL
                       END,
                       'comment_count', comment_counts.count
                   ) ORDER BY t.priority DESC, t.due_date
               ) as tasks
        FROM benchmark.tasks t
        LEFT JOIN benchmark.employees e ON t.assigned_to_id = e.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*) as count
            FROM benchmark.task_comments tc
            WHERE tc.task_id = t.id
        ) comment_counts ON true
        WHERE t.project_id = ANY($1)
        GROUP BY t.project_id
        """
        rows = await conn.fetch(query, project_ids)

        task_map = {str(row["project_id"]): row["tasks"] for row in rows}
        return [task_map.get(proj_id, []) for proj_id in project_ids]


# Initialize DataLoaders
departments_loader = DataLoader(load_departments_by_organization)
teams_loader = DataLoader(load_teams_by_department)
employees_loader = DataLoader(load_employees_by_team)
projects_loader = DataLoader(load_projects_by_department)
project_members_loader = DataLoader(load_project_members)
tasks_loader = DataLoader(load_tasks_by_project)


# GraphQL Types
@strawberry.type
class Employee:
    id: str
    email: str
    username: str
    full_name: str
    role: str
    level: int
    salary: Optional[float]
    hire_date: date
    skills: Optional[list[dict]] = None
    certifications: Optional[list[dict]] = None
    created_at: datetime


@strawberry.type
class Team:
    id: str
    name: str
    description: Optional[str]
    formation_date: Optional[date]
    is_active: bool
    performance_metrics: Optional[dict] = None

    @strawberry.field
    async def employees(self, limit: int = 10) -> list[Employee]:
        monitor.record_resolver("team.employees")
        employees_data = await employees_loader.load(self.id)
        return [Employee(**emp_data) for emp_data in employees_data[:limit]]

    @strawberry.field
    async def employee_count(self) -> int:
        monitor.record_resolver("team.employee_count")
        employees_data = await employees_loader.load(self.id)
        return len(employees_data)


@strawberry.type
class Department:
    id: str
    name: str
    code: str
    budget: Optional[float]
    head_count: int
    created_at: datetime
    updated_at: datetime

    @strawberry.field
    async def teams(self, limit: int = 10) -> list[Team]:
        monitor.record_resolver("department.teams")
        teams_data = await teams_loader.load(self.id)
        return [Team(**team_data) for team_data in teams_data[:limit]]

    @strawberry.field
    async def projects(self, limit: int = 10) -> list["Project"]:
        monitor.record_resolver("department.projects")
        projects_data = await projects_loader.load(self.id)
        return [Project(**proj_data) for proj_data in projects_data[:limit]]


@strawberry.type
class ProjectMember:
    id: str
    full_name: str
    email: str
    role: str
    allocation_percentage: int
    start_date: date
    end_date: Optional[date]


@strawberry.type
class TaskAssignee:
    id: str
    full_name: str
    email: str


@strawberry.type
class Task:
    id: str
    title: str
    description: Optional[str]
    status: str
    priority: int
    estimated_hours: Optional[float]
    actual_hours: Optional[float]
    due_date: Optional[date]
    tags: Optional[list[str]] = None
    assigned_to: Optional[TaskAssignee]
    comment_count: int


@strawberry.type
class Project:
    id: str
    name: str
    description: Optional[str]
    status: str
    priority: int
    budget: Optional[float]
    start_date: Optional[date]
    end_date: Optional[date]
    milestones: Optional[list[dict]] = None
    dependencies: Optional[list[dict]] = None
    lead_employee_id: Optional[str]
    task_count: int
    team_size: int

    @strawberry.field
    async def team_members(self, limit: int = 10) -> list[ProjectMember]:
        monitor.record_resolver("project.team_members")
        members_data = await project_members_loader.load(self.id)
        return [ProjectMember(**member_data) for member_data in members_data[:limit]]

    @strawberry.field
    async def recent_tasks(self, limit: int = 5) -> list[Task]:
        monitor.record_resolver("project.recent_tasks")
        tasks_data = await tasks_loader.load(self.id)
        return [
            Task(
                **{
                    **task_data,
                    "assigned_to": TaskAssignee(**task_data["assigned_to"])
                    if task_data["assigned_to"]
                    else None,
                }
            )
            for task_data in tasks_data[:limit]
        ]


@strawberry.type
class Organization:
    id: str
    name: str
    description: Optional[str]
    industry: str
    founded_date: Optional[date]
    headquarters_address: Optional[dict] = None
    metadata: Optional[dict] = None
    created_at: datetime
    updated_at: datetime

    @strawberry.field
    async def departments(self, limit: int = 10) -> list[Department]:
        monitor.record_resolver("organization.departments")
        departments_data = await departments_loader.load(self.id)
        return [Department(**dept_data) for dept_data in departments_data[:limit]]

    @strawberry.field
    @cached_resolver(ttl=600)  # Cache for 10 minutes
    async def department_count(self) -> int:
        monitor.record_resolver("organization.department_count")
        departments_data = await departments_loader.load(self.id)
        return len(departments_data)

    @strawberry.field
    @cached_resolver(ttl=600)
    async def employee_count(self) -> int:
        monitor.record_resolver("organization.employee_count")
        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            count = await conn.fetchval(
                """
                SELECT COUNT(*)
                FROM benchmark.employees e
                JOIN benchmark.teams t ON e.team_id = t.id
                JOIN benchmark.departments d ON t.department_id = d.id
                WHERE d.organization_id = $1
            """,
                self.id,
            )
            return count or 0

    @strawberry.field
    @cached_resolver(ttl=600)
    async def total_budget(self) -> float:
        monitor.record_resolver("organization.total_budget")
        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            budget = await conn.fetchval(
                """
                SELECT COALESCE(SUM(budget), 0)
                FROM benchmark.departments
                WHERE organization_id = $1
            """,
                self.id,
            )
            return float(budget or 0)


# Optimized root queries with efficient data fetching
@strawberry.type
class Query:
    @strawberry.field
    @cached_resolver(ttl=300)
    async def organizations(self, limit: int = 10) -> list[Organization]:
        monitor.record_resolver("query.organizations")
        monitor.record_query()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            rows = await conn.fetch(
                """
                SELECT id::text, name, description, industry, founded_date,
                       headquarters_address, metadata, created_at, updated_at
                FROM benchmark.organizations
                ORDER BY name
                LIMIT $1
            """,
                limit,
            )

            return [Organization(**dict(row)) for row in rows]

    @strawberry.field
    @cached_resolver(ttl=300)
    async def organizations_hierarchy(self, limit: int = 5) -> list[Organization]:
        """Optimized hierarchy query with DataLoaders."""
        monitor.record_resolver("query.organizations_hierarchy")
        monitor.record_query()

        # Get base organizations
        orgs = await self.organizations(limit=limit)

        # Pre-load all related data using DataLoaders
        org_ids = [org.id for org in orgs]

        # This will trigger the DataLoaders to batch-load all departments
        departments_data = await departments_loader.load_many(org_ids)

        # Extract department IDs for team loading
        dept_ids = []
        for dept_list in departments_data:
            dept_ids.extend([dept["id"] for dept in dept_list])

        # Pre-load teams for all departments
        if dept_ids:
            await teams_loader.load_many(dept_ids)

            # Extract team IDs for employee loading
            teams_data = await teams_loader.load_many(dept_ids)
            team_ids = []
            for team_list in teams_data:
                team_ids.extend([team["id"] for team in team_list])

            # Pre-load employees for all teams
            if team_ids:
                await employees_loader.load_many(team_ids)

        return orgs

    @strawberry.field
    @cached_resolver(ttl=300)
    async def projects_deep(self, statuses: list[str] = None, limit: int = 10) -> list[Project]:
        """Optimized deep project query."""
        if statuses is None:
            statuses = ["planning", "in_progress"]
        monitor.record_resolver("query.projects_deep")
        monitor.record_query()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            rows = await conn.fetch(
                """
                SELECT p.id::text, p.name, p.description, p.status, p.priority,
                       p.budget, p.start_date, p.end_date, p.milestones, p.dependencies,
                       p.lead_employee_id::text,
                       task_counts.count as task_count,
                       member_counts.count as team_size
                FROM benchmark.projects p
                LEFT JOIN LATERAL (
                    SELECT COUNT(*) as count
                    FROM benchmark.tasks t
                    WHERE t.project_id = p.id
                ) task_counts ON true
                LEFT JOIN LATERAL (
                    SELECT COUNT(*) as count
                    FROM benchmark.project_members pm
                    WHERE pm.project_id = p.id
                ) member_counts ON true
                WHERE p.status = ANY($1)
                ORDER BY p.priority DESC, p.created_at DESC
                LIMIT $2
            """,
                statuses,
                limit,
            )

            projects = [Project(**dict(row)) for row in rows]

            # Pre-load related data
            project_ids = [p.id for p in projects]
            await project_members_loader.load_many(project_ids)
            await tasks_loader.load_many(project_ids)

            return projects

    @strawberry.field
    @cached_resolver(ttl=180)
    async def enterprise_stats(self) -> dict[str, Any]:
        """Optimized aggregation query."""
        monitor.record_resolver("query.enterprise_stats")
        monitor.record_query()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            stats = await conn.fetchrow("""
                SELECT
                    (SELECT COUNT(*) FROM benchmark.organizations) as organization_count,
                    (SELECT COUNT(*) FROM benchmark.departments) as department_count,
                    (SELECT COUNT(*) FROM benchmark.teams) as team_count,
                    (SELECT COUNT(*) FROM benchmark.employees) as employee_count,
                    (SELECT COUNT(*) FROM benchmark.projects) as project_count,
                    (SELECT COUNT(*) FROM benchmark.tasks) as task_count,
                    (SELECT COALESCE(SUM(budget), 0) FROM benchmark.departments) as total_budget,
                    (SELECT COALESCE(SUM(hours), 0) FROM benchmark.time_entries) as total_hours_logged,
                    (SELECT ROUND(AVG(level), 2) FROM benchmark.employees) as avg_employee_level
            """)

            return dict(stats)

    @strawberry.field
    async def performance_stats(self) -> dict[str, Any]:
        """Get Strawberry performance statistics."""
        return monitor.get_stats()


# Mutation types for write operations
@strawberry.input
class CreateProjectInput:
    name: str
    description: str
    department_id: str
    lead_employee_id: str
    budget: float
    start_date: date
    end_date: date


@strawberry.type
class CreateProjectResult:
    project_id: str
    execution_time_ms: float


@strawberry.type
class Mutation:
    @strawberry.mutation
    async def create_project(self, input: CreateProjectInput) -> CreateProjectResult:
        """Optimized project creation mutation."""
        start_time = time.time()

        pool = await get_connection_pool()
        async with pool.acquire() as conn:
            async with conn.transaction():
                # Create project
                project_id = await conn.fetchval(
                    """
                    INSERT INTO benchmark.projects (
                        name, description, department_id, lead_employee_id,
                        budget, start_date, end_date, status, priority
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'planning', 3)
                    RETURNING id
                """,
                    input.name,
                    input.description,
                    input.department_id,
                    input.lead_employee_id,
                    input.budget,
                    input.start_date,
                    input.end_date,
                )

                # Audit log
                await conn.execute(
                    """
                    INSERT INTO benchmark.audit_log (entity_type, entity_id, action, actor_id, changes)
                    VALUES ('project', $1, 'create', $2, $3)
                """,
                    project_id,
                    input.lead_employee_id,
                    json.dumps({"name": input.name, "budget": float(input.budget)}),
                )

        execution_time = (time.time() - start_time) * 1000

        # Clear relevant caches
        redis_conn = await get_redis()
        if redis_conn:
            try:
                await redis_conn.delete("strawberry:query.projects_deep:*")
                await redis_conn.delete("strawberry:query.enterprise_stats:*")
            except Exception:
                pass

        return CreateProjectResult(project_id=str(project_id), execution_time_ms=execution_time)


# Create the schema
schema = strawberry.Schema(query=Query, mutation=Mutation)

# FastAPI app with optimized settings
app = FastAPI(
    title="Ultra-Optimized Strawberry GraphQL Benchmark",
    description="Showcasing Strawberry's best performance with DataLoaders, caching, and optimizations",
)

graphql_app = GraphQLRouter(schema, debug=False)
app.include_router(graphql_app, prefix="/graphql")


@app.on_event("startup")
async def startup_event():
    """Initialize optimizations."""
    print("🍓 Starting ultra-optimized Strawberry GraphQL...")

    # Initialize connection pool
    pool = await get_connection_pool()
    print(f"✅ Database connection pool: {pool.get_min_size()}-{pool.get_max_size()} connections")

    # Test Redis
    redis_conn = await get_redis()
    if redis_conn:
        print("✅ Redis caching enabled")
    else:
        print("⚠️  Redis not available - running without cache")

    print("🏆 Strawberry optimizations ready: DataLoaders + Connection Pooling + Caching")


@app.on_event("shutdown")
async def shutdown_event():
    """Clean up resources."""
    global connection_pool, redis_client

    if connection_pool:
        await connection_pool.close()
        print("✅ Closed database connection pool")

    if redis_client:
        await redis_client.close()
        print("✅ Closed Redis connection")


@app.get("/health")
async def health():
    """Health check with optimization status."""
    pool = await get_connection_pool()
    redis_conn = await get_redis()

    return {
        "status": "healthy",
        "framework": "Strawberry GraphQL",
        "optimizations": [
            "dataloader_n_plus_one_elimination",
            "connection_pooling",
            "redis_caching",
            "efficient_resolvers",
            "query_batching",
        ],
        "connection_pool": {
            "size": pool.get_size(),
            "idle": pool.get_idle_size(),
            "min_size": pool.get_min_size(),
            "max_size": pool.get_max_size(),
        },
        "redis_available": redis_conn is not None,
        "performance_monitor": monitor.get_stats(),
    }


@app.get("/stats")
async def stats():
    """Get performance statistics."""
    return {
        "framework": "Strawberry GraphQL",
        "performance_stats": monitor.get_stats(),
        "dataloader_status": "active",
        "caching_status": "redis" if await get_redis() else "disabled",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8001, workers=1, loop="asyncio", access_log=False)  # noqa: S104
