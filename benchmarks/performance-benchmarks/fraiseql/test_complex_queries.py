#!/usr/bin/env python3
"""
Test complex queries against the existing FraiseQL instance with the complex schema.
"""

import asyncio
import time

import asyncpg

DATABASE_URL = "postgresql://benchmark:benchmark@localhost:5432/benchmark_db"


async def test_complex_queries():
    print("🏆 Testing FraiseQL Complex Domain Queries")
    print("=" * 80)

    try:
        conn = await asyncpg.connect(DATABASE_URL)
        print("✅ Connected to PostgreSQL with complex schema")
    except Exception as e:
        print(f"❌ Failed to connect to database: {e}")
        return

    # Test 1: Organization Hierarchy Query
    print("\n1️⃣ ORGANIZATION HIERARCHY QUERY (4 levels deep)")
    print("-" * 60)

    start_time = time.time()
    try:
        query = """
        WITH RECURSIVE org_tree AS (
            SELECT
                o.id as org_id,
                o.name as org_name,
                jsonb_build_object(
                    'id', o.id::text,
                    'name', o.name,
                    'description', o.description,
                    'industry', o.industry,
                    'departments', '[]'::jsonb
                ) as data
            FROM benchmark.organizations o
            LIMIT 3
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
        LEFT JOIN benchmark.departments d ON d.organization_id = ot.org_id
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
            FROM benchmark.teams t
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM benchmark.employees e
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
                FROM benchmark.employees e
                WHERE e.team_id = t.id
                LIMIT 3
            ) emp_data ON true
            WHERE t.department_id = d.id
        ) teams ON true
        GROUP BY ot.org_id, ot.data
        """

        results = await conn.fetch(query)
        query_time = (time.time() - start_time) * 1000

        print(f"✅ Query completed in {query_time:.2f}ms")
        print(f"✅ Organizations processed: {len(results)}")

        for result in results:
            org_data = result["data"]
            print(f"   📋 {org_data['name']} ({org_data['industry']})")
            for dept in org_data.get("departments", []):
                print(f"      └── {dept['name']} (${dept['budget']:,})")
                for team in dept.get("teams", []):
                    emp_count = team["employeeCount"]
                    print(f"          └── {team['name']} ({emp_count} employees)")

    except Exception as e:
        print(f"❌ Organization hierarchy query failed: {e}")

    # Test 2: Project Full Details Query
    print("\n\n2️⃣ PROJECT FULL DETAILS QUERY (5+ levels deep)")
    print("-" * 60)

    start_time = time.time()
    try:
        query = """
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
                'department', dept_data.data,
                'leadEmployee', lead_data.data,
                'teamMembers', COALESCE(members.data, '[]'::jsonb),
                'recentTasks', COALESCE(tasks.data, '[]'::jsonb),
                'timeAnalytics', COALESCE(time_stats.data, '{}'::jsonb)
            ) as data
        FROM benchmark.projects p
        LEFT JOIN LATERAL (
            SELECT jsonb_build_object(
                'id', d.id::text,
                'name', d.name,
                'code', d.code,
                'organization', jsonb_build_object(
                    'id', o.id::text,
                    'name', o.name,
                    'industry', o.industry
                )
            ) as data
            FROM benchmark.departments d
            JOIN benchmark.organizations o ON d.organization_id = o.id
            WHERE d.id = p.department_id
        ) dept_data ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_build_object(
                'id', e.id::text,
                'fullName', e.full_name,
                'email', e.email,
                'role', e.role,
                'team', CASE
                    WHEN t.id IS NOT NULL THEN jsonb_build_object(
                        'id', t.id::text,
                        'name', t.name
                    )
                    ELSE NULL
                END
            ) as data
            FROM benchmark.employees e
            LEFT JOIN benchmark.teams t ON e.team_id = t.id
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
            FROM benchmark.project_members pm
            JOIN benchmark.employees e ON pm.employee_id = e.id
            WHERE pm.project_id = p.id
            LIMIT 5
        ) members ON true
        LEFT JOIN LATERAL (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', t.id::text,
                    'title', t.title,
                    'status', t.status,
                    'priority', t.priority,
                    'assignedTo', CASE
                        WHEN e.id IS NOT NULL THEN jsonb_build_object(
                            'id', e.id::text,
                            'fullName', e.full_name
                        )
                        ELSE NULL
                    END,
                    'dueDate', t.due_date,
                    'estimatedHours', t.estimated_hours
                )
                ORDER BY t.priority DESC, t.due_date
            ) as data
            FROM benchmark.tasks t
            LEFT JOIN benchmark.employees e ON t.assigned_to_id = e.id
            WHERE t.project_id = p.id
            LIMIT 3
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
            FROM benchmark.time_entries te
            JOIN benchmark.tasks t ON te.task_id = t.id
            WHERE t.project_id = p.id
        ) time_stats ON true
        WHERE p.status IN ('planning', 'in_progress')
        ORDER BY p.priority DESC, p.created_at DESC
        LIMIT 3
        """

        results = await conn.fetch(query)
        query_time = (time.time() - start_time) * 1000

        print(f"✅ Query completed in {query_time:.2f}ms")
        print(f"✅ Projects processed: {len(results)}")

        for result in results:
            project_data = result["data"]
            print(f"   🚀 {project_data['name']} (Priority {project_data['priority']})")
            print(f"      Status: {project_data['status']}")
            print(f"      Budget: ${project_data['budget']:,}")
            if project_data.get("department"):
                dept = project_data["department"]
                org = dept.get("organization", {})
                print(f"      Department: {dept['name']} @ {org.get('name', 'Unknown')}")
            if project_data.get("leadEmployee"):
                lead = project_data["leadEmployee"]
                print(f"      Lead: {lead['fullName']} ({lead['role']})")

            team_members = project_data.get("teamMembers", [])
            if team_members:
                print(f"      Team: {len(team_members)} members")

            tasks = project_data.get("recentTasks", [])
            if tasks:
                print(f"      Recent Tasks: {len(tasks)}")

            analytics = project_data.get("timeAnalytics", {})
            if analytics.get("totalHours", 0) > 0:
                print(
                    f"      Time Logged: {analytics['totalHours']} hours ({analytics['uniqueContributors']} contributors)"
                )

    except Exception as e:
        print(f"❌ Project details query failed: {e}")

    # Test 3: Aggregation Performance
    print("\n\n3️⃣ COMPLEX AGGREGATION QUERY")
    print("-" * 60)

    start_time = time.time()
    try:
        query = """
        SELECT
            jsonb_build_object(
                'organizationStats', org_stats.data,
                'departmentStats', dept_stats.data,
                'projectStats', proj_stats.data,
                'employeeStats', emp_stats.data
            ) as summary
        FROM (
            SELECT jsonb_build_object(
                'totalOrganizations', COUNT(*),
                'avgEmployeesPerOrg', ROUND(AVG(emp_counts.count), 2),
                'totalBudget', SUM(dept_budgets.total)
            ) as data
            FROM benchmark.organizations o
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM benchmark.employees e
                JOIN benchmark.teams t ON e.team_id = t.id
                JOIN benchmark.departments d ON t.department_id = d.id
                WHERE d.organization_id = o.id
            ) emp_counts ON true
            LEFT JOIN LATERAL (
                SELECT COALESCE(SUM(budget), 0) as total
                FROM benchmark.departments
                WHERE organization_id = o.id
            ) dept_budgets ON true
        ) org_stats,
        (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'name', d.name,
                    'employeeCount', emp_count.count,
                    'projectCount', proj_count.count,
                    'avgProjectBudget', ROUND(COALESCE(avg_budget.avg, 0), 2)
                )
            ) as data
            FROM benchmark.departments d
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM benchmark.employees e
                JOIN benchmark.teams t ON e.team_id = t.id
                WHERE t.department_id = d.id
            ) emp_count ON true
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM benchmark.projects p
                WHERE p.department_id = d.id
            ) proj_count ON true
            LEFT JOIN LATERAL (
                SELECT AVG(budget) as avg
                FROM benchmark.projects p
                WHERE p.department_id = d.id
            ) avg_budget ON true
        ) dept_stats,
        (
            SELECT jsonb_build_object(
                'totalProjects', COUNT(*),
                'statusBreakdown', status_breakdown.data,
                'avgTasksPerProject', ROUND(AVG(task_counts.count), 2)
            ) as data
            FROM benchmark.projects p
            LEFT JOIN LATERAL (
                SELECT COUNT(*) as count
                FROM benchmark.tasks t
                WHERE t.project_id = p.id
            ) task_counts ON true,
            (
                SELECT jsonb_object_agg(status, count) as data
                FROM (
                    SELECT status, COUNT(*) as count
                    FROM benchmark.projects
                    GROUP BY status
                ) status_counts
            ) status_breakdown
        ) proj_stats,
        (
            SELECT jsonb_build_object(
                'totalEmployees', COUNT(*),
                'avgLevel', ROUND(AVG(level), 2),
                'roleDistribution', role_dist.data,
                'avgSkillsPerEmployee', ROUND(AVG(jsonb_array_length(skills)), 2)
            ) as data
            FROM benchmark.employees e,
            (
                SELECT jsonb_object_agg(role, count) as data
                FROM (
                    SELECT role, COUNT(*) as count
                    FROM benchmark.employees
                    GROUP BY role
                    ORDER BY count DESC
                    LIMIT 5
                ) role_counts
            ) role_dist
        ) emp_stats
        """

        result = await conn.fetchrow(query)
        query_time = (time.time() - start_time) * 1000

        print(f"✅ Complex aggregation completed in {query_time:.2f}ms")

        if result:
            summary = result["summary"]
            print("\n📊 Enterprise Summary:")

            org_stats = summary.get("organizationStats", {})
            print(f"   Organizations: {org_stats.get('totalOrganizations', 0)}")
            print(f"   Avg Employees/Org: {org_stats.get('avgEmployeesPerOrg', 0)}")
            print(f"   Total Budget: ${org_stats.get('totalBudget', 0):,}")

            proj_stats = summary.get("projectStats", {})
            print(f"   Projects: {proj_stats.get('totalProjects', 0)}")
            print(f"   Avg Tasks/Project: {proj_stats.get('avgTasksPerProject', 0)}")

            emp_stats = summary.get("employeeStats", {})
            print(f"   Employees: {emp_stats.get('totalEmployees', 0)}")
            print(f"   Avg Level: {emp_stats.get('avgLevel', 0)}")
            print(f"   Avg Skills/Employee: {emp_stats.get('avgSkillsPerEmployee', 0)}")

    except Exception as e:
        print(f"❌ Aggregation query failed: {e}")

    await conn.close()

    print("\n" + "=" * 80)
    print("💡 Performance Summary:")
    print("✅ Single SQL queries handle deep nesting (4-5 levels)")
    print("✅ JSONB aggregation builds nested structures in database")
    print("✅ Lateral joins prevent N+1 query problems")
    print("✅ Complex aggregations computed at database level")
    print("✅ Results are pre-formatted JSON matching GraphQL schema")
    print("=" * 80)


if __name__ == "__main__":
    asyncio.run(test_complex_queries())
