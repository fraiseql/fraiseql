"""Ultra-optimized FraiseQL with complex domain models and mutations."""

import os
import time
from collections import deque
from datetime import date
from decimal import Decimal
from typing import Any, Optional

import asyncpg
import redis.asyncio as redis
from fastapi import FastAPI, HTTPException
from pydantic import UUID4, BaseModel

app = FastAPI(title="Ultra-Optimized FraiseQL Complex Domain Benchmark")

# Database configuration
DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://benchmark:benchmark@postgres-bench:5432/benchmark_db"
)

# Global connection pools
connection_pools: dict[str, asyncpg.Pool] = {}
redis_pool: Optional[redis.ConnectionPool] = None
redis_client: Optional[redis.Redis] = None


# Performance monitoring
class PerformanceMonitor:
    def __init__(self):
        self.request_count = 0
        self.cache_hits = 0
        self.mutation_count = 0
        self.complex_query_count = 0
        self.pool_stats = {"read": 0, "write": 0, "hot": 0}

    def record_request(
        self,
        pool_type: str = "read",
        cache_hit: bool = False,
        is_mutation: bool = False,
        is_complex: bool = False,
    ):
        self.request_count += 1
        self.pool_stats[pool_type] += 1
        if cache_hit:
            self.cache_hits += 1
        if is_mutation:
            self.mutation_count += 1
        if is_complex:
            self.complex_query_count += 1

    def get_stats(self):
        return {
            "total_requests": self.request_count,
            "cache_hit_rate": (self.cache_hits / max(1, self.request_count)) * 100,
            "mutation_count": self.mutation_count,
            "complex_query_count": self.complex_query_count,
            "pool_usage": self.pool_stats,
        }


monitor = PerformanceMonitor()


# Multi-level cache
class MultiLevelCache:
    def __init__(self):
        self.l1_cache = {}
        self.l1_order = deque(maxlen=5000)  # Larger cache for complex objects
        self.l1_max_size = 5000

    def _evict_l1_if_needed(self):
        while len(self.l1_cache) >= self.l1_max_size and self.l1_order:
            oldest_key = self.l1_order.popleft()
            self.l1_cache.pop(oldest_key, None)

    def l1_get(self, key: str):
        if key in self.l1_cache:
            self.l1_order.append(key)
            return self.l1_cache[key]
        return None

    def l1_set(self, key: str, value: Any):
        self._evict_l1_if_needed()
        self.l1_cache[key] = value
        self.l1_order.append(key)

    def l1_invalidate_pattern(self, pattern: str):
        """Invalidate cache entries matching a pattern (for mutations)."""
        keys_to_remove = [k for k in self.l1_cache if pattern in k]
        for key in keys_to_remove:
            self.l1_cache.pop(key, None)


cache = MultiLevelCache()


# Pydantic models for mutations
class CreateProjectInput(BaseModel):
    name: str
    description: str
    department_id: UUID4
    lead_employee_id: UUID4
    budget: Decimal
    start_date: date
    end_date: date


class AssignEmployeeInput(BaseModel):
    project_id: UUID4
    employee_id: UUID4
    role: str
    allocation_percentage: int


class UpdateTaskStatusInput(BaseModel):
    task_id: UUID4
    new_status: str
    actor_id: UUID4


async def setup_connection(conn):
    """Optimize each connection for complex queries."""
    await conn.execute("SET work_mem = '32MB'")  # More memory for complex joins
    await conn.execute("SET join_collapse_limit = 12")  # Allow more join planning
    await conn.execute("SET from_collapse_limit = 12")
    await conn.execute("SET gin_fuzzy_search_limit = 0")
    await conn.execute("SET search_path = benchmark, public")
    await conn.execute("SET synchronous_commit = off")


async def get_connection_pools():
    """Initialize multi-tier connection pools."""
    global connection_pools

    if not connection_pools:
        # Read pool - optimized for complex queries
        connection_pools["read"] = await asyncpg.create_pool(
            DATABASE_URL,
            min_size=10,
            max_size=30,
            max_queries=10000,
            max_inactive_connection_lifetime=300,
            command_timeout=30,  # Longer timeout for complex queries
            setup=setup_connection,
            server_settings={"jit": "off", "application_name": "fraiseql_complex_read_pool"},
        )

        # Write pool - for mutations
        connection_pools["write"] = await asyncpg.create_pool(
            DATABASE_URL,
            min_size=5,
            max_size=15,
            max_queries=5000,
            max_inactive_connection_lifetime=300,
            command_timeout=30,
            setup=setup_connection,
            server_settings={"jit": "off", "application_name": "fraiseql_complex_write_pool"},
        )

        # Hot queries pool
        connection_pools["hot"] = await asyncpg.create_pool(
            DATABASE_URL,
            min_size=5,
            max_size=20,
            max_queries=20000,
            max_inactive_connection_lifetime=600,
            command_timeout=10,
            setup=setup_connection,
            server_settings={"jit": "off", "application_name": "fraiseql_complex_hot_pool"},
        )

    return connection_pools


async def get_redis():
    """Get Redis client with connection pooling."""
    global redis_pool, redis_client

    if redis_client is None:
        try:
            if redis_pool is None:
                redis_pool = redis.ConnectionPool(
                    host=os.environ.get("REDIS_HOST", "localhost"),
                    port=int(os.environ.get("REDIS_PORT", "6379")),
                    max_connections=50,
                    retry_on_timeout=True,
                    socket_keepalive=True,
                    health_check_interval=30,
                )

            redis_client = redis.Redis(
                connection_pool=redis_pool,
                decode_responses=True,
                socket_connect_timeout=5,
                socket_timeout=5,
            )
        except Exception as e:
            print(f"Redis connection failed: {e}")
            redis_client = None

    return redis_client


# Complex query definitions
COMPLEX_QUERIES = {
    "organization_full": """
        SELECT data FROM tv_organization_full
        WHERE id = ANY($1::uuid[])
        LIMIT $2
    """,
    "project_deep": """
        SELECT data FROM tv_project_deep
        WHERE (data->>'status') = ANY($1::text[])
        ORDER BY ((data->>'priority')::int) DESC
        LIMIT $2
    """,
    "organization_hierarchy_deep": """
        WITH RECURSIVE org_tree AS (
            SELECT
                o.id as org_id,
                o.name as org_name,
                o.metadata,
                jsonb_build_object(
                    'id', o.id::text,
                    'name', o.name,
                    'description', o.description,
                    'industry', o.industry,
                    'foundedDate', o.founded_date,
                    'departments', '[]'::jsonb
                ) as data
            FROM organizations o
            WHERE o.id = ANY($1::uuid[])
        )
        SELECT
            ot.org_id,
            jsonb_set(
                ot.data,
                '{departments}',
                COALESCE(
                    jsonb_agg(
                        jsonb_build_object(
                            'id', d.id::text,
                            'name', d.name,
                            'code', d.code,
                            'budget', d.budget,
                            'teams', COALESCE(teams.teams_data, '[]'::jsonb)
                        )
                    ) FILTER (WHERE d.id IS NOT NULL),
                    '[]'::jsonb
                )
            ) as data
        FROM org_tree ot
        LEFT JOIN departments d ON d.organization_id = ot.org_id
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', t.id::text,
                    'name', t.name,
                    'description', t.description,
                    'isActive', t.is_active,
                    'employeeCount', emp_counts.count,
                    'employees', COALESCE(emp_data.employees, '[]'::jsonb)
                )
            ) as teams_data
            FROM teams t
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM employees e
                WHERE e.team_id = t.id
            ) emp_counts ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', e.id::text,
                        'fullName', e.full_name,
                        'email', e.email,
                        'role', e.role,
                        'level', e.level,
                        'skills', e.skills
                    )
                    ORDER BY e.level DESC, e.full_name
                ) as employees
                FROM employees e
                WHERE e.team_id = t.id
                LIMIT 5
            ) emp_data ON true
            WHERE t.department_id = d.id
        ) teams ON true
        GROUP BY ot.org_id, ot.data
        LIMIT $2
    """,
    "project_with_full_details": """
        SELECT
            p.id,
            jsonb_build_object(
                'id', p.id::text,
                'name', p.name,
                'description', p.description,
                'status', p.status,
                'priority', p.priority,
                'budget', p.budget,
                'startDate', p.start_date,
                'endDate', p.end_date,
                'milestones', p.milestones,
                'dependencies', p.dependencies,
                'department', dept_data.data,
                'leadEmployee', lead_data.data,
                'teamMembers', COALESCE(members.data, '[]'::jsonb),
                'recentTasks', COALESCE(tasks.data, '[]'::jsonb),
                'timeAnalytics', COALESCE(time_stats.data, '{}'::jsonb),
                'documents', COALESCE(docs.data, '[]'::jsonb)
            ) as data
        FROM projects p
        LEFT JOIN LATERAL (
            SELECT jsonb_build_object(
                'id', d.id::text,
                'name', d.name,
                'code', d.code,
                'organization', jsonb_build_object(
                    'id', o.id::text,
                    'name', o.name
                )
            ) as data
            FROM departments d
            JOIN organizations o ON d.organization_id = o.id
            WHERE d.id = p.department_id
        ) dept_data ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_build_object(
                'id', e.id::text,
                'fullName', e.full_name,
                'email', e.email,
                'role', e.role,
                'team', jsonb_build_object(
                    'id', t.id::text,
                    'name', t.name
                )
            ) as data
            FROM employees e
            LEFT JOIN teams t ON e.team_id = t.id
            WHERE e.id = p.lead_employee_id
        ) lead_data ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', e.id::text,
                    'fullName', e.full_name,
                    'role', pm.role,
                    'allocation', pm.allocation_percentage,
                    'startDate', pm.start_date
                )
                ORDER BY pm.allocation_percentage DESC
            ) as data
            FROM project_members pm
            JOIN employees e ON pm.employee_id = e.id
            WHERE pm.project_id = p.id
            LIMIT 10
        ) members ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', t.id::text,
                    'title', t.title,
                    'status', t.status,
                    'priority', t.priority,
                    'assignedTo', jsonb_build_object(
                        'id', e.id::text,
                        'fullName', e.full_name
                    ),
                    'dueDate', t.due_date,
                    'commentCount', comment_counts.count
                )
                ORDER BY t.priority DESC, t.due_date
            ) as data
            FROM tasks t
            LEFT JOIN employees e ON t.assigned_to_id = e.id
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM task_comments tc
                WHERE tc.task_id = t.id
            ) comment_counts ON true
            WHERE t.project_id = p.id
            LIMIT 5
        ) tasks ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_build_object(
                'totalHours', COALESCE(SUM(te.hours), 0),
                'billableHours', COALESCE(SUM(te.hours) FILTER (WHERE te.billable), 0),
                'uniqueContributors', COUNT(DISTINCT te.employee_id),
                'averageHoursPerTask',
                    CASE
                        WHEN COUNT(DISTINCT te.task_id) > 0
                        THEN ROUND(SUM(te.hours) / COUNT(DISTINCT te.task_id), 2)
                        ELSE 0
                    END
            ) as data
            FROM time_entries te
            JOIN tasks t ON te.task_id = t.id
            WHERE t.project_id = p.id
        ) time_stats ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', d.id::text,
                    'title', d.title,
                    'status', d.status,
                    'version', d.version,
                    'author', jsonb_build_object(
                        'id', e.id::text,
                        'fullName', e.full_name
                    ),
                    'updatedAt', d.updated_at
                )
                ORDER BY d.updated_at DESC
            ) as data
            FROM documents d
            JOIN employees e ON d.author_id = e.id
            WHERE d.project_id = p.id
            LIMIT 3
        ) docs ON true
        WHERE p.id = ANY($1::uuid[])
        LIMIT $2
    """,
}


@app.on_event("startup")
async def startup_event():
    """Initialize all optimizations on startup."""
    print("🚀 Starting ultra-optimized FraiseQL complex domain benchmark app...")

    await get_connection_pools()
    print("✅ Database connection pools initialized for complex queries")

    redis_conn = await get_redis()
    if redis_conn:
        try:
            await redis_conn.ping()
            print("✅ Redis connection established")
        except Exception as e:
            print(f"⚠️  Redis connection failed: {e}")

    print("🏆 Ready for complex domain benchmarking with mutations")


@app.on_event("shutdown")
async def shutdown_event():
    """Clean up all resources."""
    global connection_pools, redis_client, redis_pool

    for pool in connection_pools.values():
        if pool:
            await pool.close()

    if redis_client:
        await redis_client.close()
    if redis_pool:
        await redis_pool.disconnect()


@app.get("/health")
async def health():
    """Health check endpoint."""
    pools = await get_connection_pools()
    pool_status = {}
    for name, pool in pools.items():
        pool_status[name] = {"size": pool.get_size(), "idle": pool.get_idle_size()}

    return {
        "status": "healthy",
        "connection_pools": pool_status,
        "performance_monitor": monitor.get_stats(),
    }


@app.get("/benchmark/organizations/simple")
async def benchmark_organizations_simple(limit: int = 10):
    """Simple organization query (baseline)."""
    start_time = time.time()

    cache_key = f"orgs_simple:{limit}"
    cached = cache.l1_get(cache_key)
    if cached:
        monitor.record_request("hot", cache_hit=True)
        return cached

    pools = await get_connection_pools()
    async with pools["read"].acquire() as conn:
        results = await conn.fetch("SELECT data FROM tv_organization_full LIMIT $1", limit)

    monitor.record_request("read")

    result = {
        "query": "organizations_simple",
        "limit": limit,
        "query_time_ms": (time.time() - start_time) * 1000,
        "result_count": len(results),
    }

    cache.l1_set(cache_key, result)
    return result


@app.get("/benchmark/organizations/hierarchy")
async def benchmark_organizations_hierarchy(org_ids: str = None, limit: int = 5):
    """Complex hierarchical organization query with deep nesting."""
    start_time = time.time()

    # Parse org IDs
    if org_ids:
        org_id_list = org_ids.split(",")
    else:
        # Get random org IDs
        pools = await get_connection_pools()
        async with pools["read"].acquire() as conn:
            org_records = await conn.fetch(
                "SELECT id FROM organizations ORDER BY random() LIMIT $1", limit
            )
            org_id_list = [str(r["id"]) for r in org_records]

    cache_key = f"orgs_hierarchy:{','.join(sorted(org_id_list))}"
    cached = cache.l1_get(cache_key)
    if cached:
        monitor.record_request("hot", cache_hit=True, is_complex=True)
        return cached

    query_start = time.time()
    pools = await get_connection_pools()
    async with pools["read"].acquire() as conn:
        results = await conn.fetch(
            COMPLEX_QUERIES["organization_hierarchy_deep"], org_id_list, limit
        )
    query_time = (time.time() - query_start) * 1000

    monitor.record_request("read", is_complex=True)

    result = {
        "query": "organizations_hierarchy_deep",
        "org_ids": org_id_list,
        "limit": limit,
        "query_time_ms": query_time,
        "total_time_ms": (time.time() - start_time) * 1000,
        "result_count": len(results),
        "nesting_levels": 4,  # org -> dept -> team -> employees
    }

    cache.l1_set(cache_key, result)
    return result


@app.get("/benchmark/projects/deep")
async def benchmark_projects_deep(statuses: str = "planning,in_progress", limit: int = 10):
    """Deep project query with all relationships."""
    start_time = time.time()

    status_list = statuses.split(",")
    cache_key = f"projects_deep:{statuses}:{limit}"

    cached = cache.l1_get(cache_key)
    if cached:
        monitor.record_request("hot", cache_hit=True, is_complex=True)
        return cached

    pools = await get_connection_pools()
    async with pools["read"].acquire() as conn:
        results = await conn.fetch(COMPLEX_QUERIES["project_deep"], status_list, limit)

    monitor.record_request("read", is_complex=True)

    result = {
        "query": "projects_deep",
        "statuses": status_list,
        "limit": limit,
        "query_time_ms": (time.time() - start_time) * 1000,
        "result_count": len(results),
    }

    cache.l1_set(cache_key, result)
    return result


@app.get("/benchmark/projects/full-details")
async def benchmark_projects_full_details(project_ids: str = None, limit: int = 5):
    """Ultra-complex project query with all nested relationships."""
    start_time = time.time()

    # Get project IDs
    if project_ids:
        project_id_list = project_ids.split(",")
    else:
        pools = await get_connection_pools()
        async with pools["read"].acquire() as conn:
            project_records = await conn.fetch(
                "SELECT id FROM projects WHERE status IN ('in_progress', 'planning') ORDER BY priority DESC LIMIT $1",
                limit,
            )
            project_id_list = [str(r["id"]) for r in project_records]

    cache_key = f"projects_full:{','.join(sorted(project_id_list))}"
    cached = cache.l1_get(cache_key)
    if cached:
        monitor.record_request("hot", cache_hit=True, is_complex=True)
        return cached

    query_start = time.time()
    pools = await get_connection_pools()
    async with pools["read"].acquire() as conn:
        results = await conn.fetch(
            COMPLEX_QUERIES["project_with_full_details"], project_id_list, limit
        )
    query_time = (time.time() - query_start) * 1000

    monitor.record_request("read", is_complex=True)

    result = {
        "query": "projects_full_details",
        "project_ids": project_id_list,
        "limit": limit,
        "query_time_ms": query_time,
        "total_time_ms": (time.time() - start_time) * 1000,
        "result_count": len(results),
        "nesting_levels": 5,  # project -> dept -> org, members, tasks -> comments, docs
        "relationships_included": [
            "department",
            "organization",
            "lead_employee",
            "team_members",
            "recent_tasks",
            "task_comments",
            "time_analytics",
            "documents",
        ],
    }

    cache.l1_set(cache_key, result)
    return result


@app.post("/benchmark/mutations/create-project")
async def benchmark_create_project(project: CreateProjectInput):
    """Benchmark project creation mutation."""
    start_time = time.time()

    pools = await get_connection_pools()
    async with pools["write"].acquire() as conn:
        project_id = await conn.fetchval(
            "SELECT create_project($1, $2, $3, $4, $5, $6, $7)",
            project.name,
            project.description,
            project.department_id,
            project.lead_employee_id,
            project.budget,
            project.start_date,
            project.end_date,
        )

    # Invalidate related caches
    cache.l1_invalidate_pattern("projects")
    cache.l1_invalidate_pattern("orgs_hierarchy")

    # Also invalidate Redis if available
    redis_conn = await get_redis()
    if redis_conn:
        try:
            pattern = "projects*"
            async for key in redis_conn.scan_iter(match=pattern):
                await redis_conn.delete(key)
        except Exception:
            pass

    monitor.record_request("write", is_mutation=True)

    return {
        "mutation": "create_project",
        "project_id": str(project_id),
        "execution_time_ms": (time.time() - start_time) * 1000,
        "cache_invalidated": True,
    }


@app.post("/benchmark/mutations/assign-employee")
async def benchmark_assign_employee(assignment: AssignEmployeeInput):
    """Benchmark employee assignment mutation."""
    start_time = time.time()

    pools = await get_connection_pools()
    async with pools["write"].acquire() as conn:
        member_id = await conn.fetchval(
            "SELECT assign_employee_to_project($1, $2, $3, $4)",
            assignment.project_id,
            assignment.employee_id,
            assignment.role,
            assignment.allocation_percentage,
        )

    # Invalidate caches
    cache.l1_invalidate_pattern(f"projects_full:{assignment.project_id}")
    cache.l1_invalidate_pattern("projects_deep")

    monitor.record_request("write", is_mutation=True)

    return {
        "mutation": "assign_employee",
        "member_id": str(member_id),
        "execution_time_ms": (time.time() - start_time) * 1000,
        "cache_invalidated": True,
    }


@app.post("/benchmark/mutations/update-task-status")
async def benchmark_update_task_status(update: UpdateTaskStatusInput):
    """Benchmark task status update mutation."""
    start_time = time.time()

    pools = await get_connection_pools()
    async with pools["write"].acquire() as conn:
        success = await conn.fetchval(
            "SELECT update_task_status($1, $2, $3)",
            update.task_id,
            update.new_status,
            update.actor_id,
        )

    # Get project ID for cache invalidation
    async with pools["read"].acquire() as conn:
        project_id = await conn.fetchval(
            "SELECT project_id FROM tasks WHERE id = $1", update.task_id
        )

    if project_id:
        cache.l1_invalidate_pattern(f"projects_full:{project_id}")

    monitor.record_request("write", is_mutation=True)

    return {
        "mutation": "update_task_status",
        "success": success,
        "execution_time_ms": (time.time() - start_time) * 1000,
        "cache_invalidated": True,
    }


@app.post("/benchmark/mutations/batch-create-tasks")
async def benchmark_batch_create_tasks(project_id: str, count: int = 10):
    """Benchmark batch task creation."""
    start_time = time.time()

    pools = await get_connection_pools()

    # Get a random employee from the project
    async with pools["read"].acquire() as conn:
        employee_ids = await conn.fetch(
            """
            SELECT employee_id
            FROM project_members
            WHERE project_id = $1
            LIMIT 5
            """,
            project_id,
        )

    if not employee_ids:
        raise HTTPException(status_code=400, detail="No employees found for project")

    # Create tasks in batch
    task_ids = []
    async with pools["write"].acquire() as conn, conn.transaction():
        for i in range(count):
            task_id = await conn.fetchval(
                """
                    INSERT INTO tasks (
                        project_id, assigned_to_id, title, description,
                        status, priority, estimated_hours, due_date
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7,
                        CURRENT_DATE + INTERVAL '30 days'
                    ) RETURNING id
                    """,
                project_id,
                employee_ids[i % len(employee_ids)]["employee_id"],
                f"Batch Task {i + 1} - {time.time()}",
                f"Description for batch task {i + 1}",
                "todo",
                (i % 5) + 1,
                8.0,
            )
            task_ids.append(str(task_id))

    # Invalidate caches
    cache.l1_invalidate_pattern(f"projects_full:{project_id}")

    monitor.record_request("write", is_mutation=True)

    return {
        "mutation": "batch_create_tasks",
        "task_count": count,
        "task_ids": task_ids,
        "execution_time_ms": (time.time() - start_time) * 1000,
        "avg_time_per_task_ms": ((time.time() - start_time) * 1000) / count,
    }


@app.get("/benchmark/stats")
async def benchmark_stats():
    """Get comprehensive benchmark statistics."""
    pools = await get_connection_pools()

    # Get database statistics
    db_stats = {}
    async with pools["read"].acquire() as conn:
        for table in [
            "organizations",
            "departments",
            "teams",
            "employees",
            "projects",
            "tasks",
            "task_comments",
            "time_entries",
        ]:
            count = await conn.fetchval(f"SELECT COUNT(*) FROM {table}")
            db_stats[table] = count

    return {
        "performance_stats": monitor.get_stats(),
        "cache_stats": {"l1_size": len(cache.l1_cache), "l1_max_size": cache.l1_max_size},
        "database_stats": db_stats,
        "connection_pools": {
            name: {"size": pool.get_size(), "idle": pool.get_idle_size()}
            for name, pool in pools.items()
        },
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        app,
        host="0.0.0.0",  # noqa: S104
        port=8000,
        workers=1,
        loop="asyncio",
        http="httptools",
        access_log=False,
    )
