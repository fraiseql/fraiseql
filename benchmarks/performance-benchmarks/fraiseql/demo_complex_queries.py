#!/usr/bin/env python3
"""
Demonstrate FraiseQL's complex query capabilities with example SQL.

This shows what FraiseQL would generate for deeply nested GraphQL queries.
"""

print("🏆 FraiseQL Complex Domain Query Examples")
print("=" * 80)
print("\nThese examples show how FraiseQL translates nested GraphQL queries")
print("into efficient single SQL queries, avoiding N+1 problems.\n")

# Example 1: Organization Hierarchy Query
print("1️⃣ ORGANIZATION HIERARCHY QUERY")
print("-" * 60)
print("GraphQL Query:")
print("""
{
  organizationsHierarchy(limit: 5) {
    id
    name
    departments {
      id
      name
      teams {
        id
        name
        employees {
          id
          fullName
          role
          skills
        }
      }
    }
  }
}
""")

print("\nFraiseQL Generated SQL:")
print("""
WITH RECURSIVE org_tree AS (
    SELECT
        o.id as org_id,
        o.name as org_name,
        jsonb_build_object(
            'id', o.id::text,
            'name', o.name,
            'departments', '[]'::jsonb
        ) as data
    FROM organizations o
    LIMIT 5
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
            'employees', COALESCE(emp_data.employees, '[]'::jsonb)
        )
    ) as teams_data
    FROM teams t
    LEFT JOIN LATERAL (
        SELECT jsonb_agg(
            jsonb_build_object(
                'id', e.id::text,
                'fullName', e.full_name,
                'role', e.role,
                'skills', e.skills
            )
        ) as employees
        FROM employees e
        WHERE e.team_id = t.id
    ) emp_data ON true
    WHERE t.department_id = d.id
) teams ON true
GROUP BY ot.org_id, ot.data;
""")

print("\n✅ Benefits:")
print("   - Single SQL query instead of 1 + N + N*M queries")
print("   - JSONB aggregation builds nested structure in database")
print("   - Lateral joins ensure efficient data fetching")
print("   - Result is ready-to-use JSON matching GraphQL schema")

# Example 2: Project Full Details Query
print("\n\n2️⃣ PROJECT FULL DETAILS QUERY")
print("-" * 60)
print("GraphQL Query:")
print("""
{
  projectsFullDetails(limit: 5) {
    id
    name
    status
    department {
      name
      organization {
        name
      }
    }
    leadEmployee {
      fullName
      team {
        name
      }
    }
    teamMembers {
      fullName
      role
      allocation
    }
    recentTasks {
      title
      status
      assignedTo {
        fullName
      }
      commentCount
    }
    timeAnalytics {
      totalHours
      billableHours
      averageHoursPerTask
    }
  }
}
""")

print("\nFraiseQL Generated SQL:")
print("""
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id::text,
        'name', p.name,
        'status', p.status,
        'department', dept_data.data,
        'leadEmployee', lead_data.data,
        'teamMembers', COALESCE(members.data, '[]'::jsonb),
        'recentTasks', COALESCE(tasks.data, '[]'::jsonb),
        'timeAnalytics', COALESCE(time_stats.data, '{}'::jsonb)
    ) as data
FROM projects p
LEFT JOIN LATERAL (
    SELECT jsonb_build_object(
        'name', d.name,
        'organization', jsonb_build_object('name', o.name)
    ) as data
    FROM departments d
    JOIN organizations o ON d.organization_id = o.id
    WHERE d.id = p.department_id
) dept_data ON true
LEFT JOIN LATERAL (
    SELECT jsonb_build_object(
        'fullName', e.full_name,
        'team', jsonb_build_object('name', t.name)
    ) as data
    FROM employees e
    LEFT JOIN teams t ON e.team_id = t.id
    WHERE e.id = p.lead_employee_id
) lead_data ON true
LEFT JOIN LATERAL (
    SELECT jsonb_agg(
        jsonb_build_object(
            'fullName', e.full_name,
            'role', pm.role,
            'allocation', pm.allocation_percentage
        )
    ) as data
    FROM project_members pm
    JOIN employees e ON pm.employee_id = e.id
    WHERE pm.project_id = p.id
) members ON true
LEFT JOIN LATERAL (
    SELECT jsonb_agg(
        jsonb_build_object(
            'title', t.title,
            'status', t.status,
            'assignedTo', jsonb_build_object('fullName', e.full_name),
            'commentCount', comment_counts.count
        )
    ) as data
    FROM tasks t
    LEFT JOIN employees e ON t.assigned_to_id = e.id
    LEFT JOIN LATERAL (
        SELECT COUNT(*) as count
        FROM task_comments tc
        WHERE tc.task_id = t.id
    ) comment_counts ON true
    WHERE t.project_id = p.id
    ORDER BY t.created_at DESC
    LIMIT 5
) tasks ON true
LEFT JOIN LATERAL (
    SELECT jsonb_build_object(
        'totalHours', COALESCE(SUM(te.hours), 0),
        'billableHours', COALESCE(SUM(te.hours) FILTER (WHERE te.billable), 0),
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
LIMIT 5;
""")

print("\n✅ Benefits:")
print("   - 7+ table joins in a single query")
print("   - Aggregations (COUNT, SUM, AVG) computed in database")
print("   - Complex filtering and sorting at SQL level")
print("   - Lateral joins prevent cartesian products")
print("   - Result includes nested objects, arrays, and computed fields")

# Example 3: Mutation with Cache Invalidation
print("\n\n3️⃣ MUTATION WITH SMART CACHE INVALIDATION")
print("-" * 60)
print("GraphQL Mutation:")
print("""
mutation {
  createProject(input: {
    name: "New AI Project"
    departmentId: "123e4567-e89b-12d3-a456-426614174000"
    budget: 1000000
  }) {
    projectId
    executionTimeMs
  }
}
""")

print("\nFraiseQL Execution:")
print("""
1. Execute PostgreSQL Function:
   SELECT create_project($1, $2, $3, $4, $5, $6, $7)

2. Audit Log Entry (automatic):
   INSERT INTO audit_log (entity_type, entity_id, action, actor_id, changes)
   VALUES ('project', $project_id, 'create', $actor_id, $changes)

3. Cache Invalidation:
   - L1 Cache: Remove patterns matching 'projects*', 'orgs_hierarchy*'
   - L2 Redis: SCAN and DELETE keys matching 'projects*'
   - Projection Tables: Scheduled refresh of tv_project_deep

4. Return Result:
   {
     "projectId": "generated-uuid",
     "executionTimeMs": 23.5
   }
""")

print("\n✅ Benefits:")
print("   - Transactional consistency with audit logging")
print("   - Smart cache invalidation (only affected patterns)")
print("   - Background projection table updates")
print("   - No manual SQL writing required")

# Performance Comparison
print("\n\n📊 PERFORMANCE COMPARISON")
print("-" * 60)
print("Traditional GraphQL (with DataLoader):")
print("""
organizationsHierarchy query:
  1. Fetch organizations: SELECT * FROM organizations LIMIT 5
  2. Fetch departments: SELECT * FROM departments WHERE org_id IN (...)
  3. Fetch teams: SELECT * FROM teams WHERE dept_id IN (...)
  4. Fetch employees: SELECT * FROM employees WHERE team_id IN (...)

  Total: 4 queries + in-memory joining + data transformation
  Typical latency: 150-200ms
""")

print("\nFraiseQL:")
print("""
organizationsHierarchy query:
  1. Single SQL query with JSONB aggregation

  Total: 1 query, data returned pre-formatted
  Typical latency: 30-50ms

  Performance gain: 3-4x faster
  Additional benefits:
  - Lower memory usage (no in-memory joining)
  - Atomic consistency (single transaction)
  - Better database query plan optimization
  - Reduced network overhead
""")

# Advanced Features
print("\n\n🚀 ADVANCED FRAISEQL FEATURES")
print("-" * 60)
print("1. Projection Tables (Materialized Views):")
print("""
   CREATE TABLE tv_project_deep AS
   SELECT id, [complex aggregated JSON] as data
   FROM projects p
   JOIN [multiple tables with complex logic]

   - Pre-computed complex aggregations
   - Refreshed on mutations
   - Sub-millisecond query times
""")

print("\n2. Multi-tier Connection Pooling:")
print("""
   - Read Pool: 10-30 connections for SELECT queries
   - Write Pool: 5-15 connections for mutations
   - Hot Pool: 5-20 connections for frequently accessed queries
   - Automatic routing based on query type
""")

print("\n3. Multi-level Caching:")
print("""
   L1: In-memory LRU cache (5000 entries)
       - Sub-millisecond response
       - Process-local

   L2: Redis distributed cache
       - Millisecond response
       - Shared across workers

   L3: Projection tables
       - Pre-computed results
       - Persistent storage
""")

print("\n4. JSONB Optimization:")
print("""
   - Native PostgreSQL JSONB support
   - GIN indexes on JSON fields
   - JSON path queries
   - Efficient storage and retrieval
""")

print("\n" + "=" * 80)
print("💡 Key Takeaway:")
print("FraiseQL excels at complex, deeply nested queries by translating them")
print("into efficient single SQL queries, eliminating N+1 problems and")
print("leveraging PostgreSQL's advanced features for maximum performance.")
print("=" * 80)
