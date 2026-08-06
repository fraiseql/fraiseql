-- Sample `test` schema seed data — the ONE seed for the shared sample entities.
--
-- Populates the sample tables with realistic data. Follows fraiseql naming
-- conventions:
--   tb_{entity} - command side table (storage)
--   v_{entity}  - canonical entity view (JSON data plane)
--
-- IDEMPOTENT BY CONSTRUCTION (#996): every row carries a FIXED id, so the
-- `ON CONFLICT (id) DO NOTHING` clauses actually fire and re-running this script
-- is a no-op. It previously used `gen_random_uuid()`, which made every id unique
-- and the conflict clauses dead — each application appended another full copy of
-- the seed. Under `cargo nextest` (a process per test) that reached 33 copies in
-- one workspace run, breaking every consumer that asserts a row count or an
-- ordering. Keep the ids fixed.

-- Ensure schema exists
CREATE SCHEMA IF NOT EXISTS test;

-- ============================================================================
-- Seed Data for project (Simple JSON structure)
-- ============================================================================

INSERT INTO test.tb_project (id, data) VALUES
  ('a0000000-0000-4000-8000-000000000001'::uuid, '{"name": "Alpha Project", "status": "active", "priority": "high"}'),
  ('a0000000-0000-4000-8000-000000000002'::uuid, '{"name": "Beta Project", "status": "archived", "priority": "low"}'),
  ('a0000000-0000-4000-8000-000000000003'::uuid, '{"name": "Gamma Project", "status": "active", "priority": "medium"}'),
  ('a0000000-0000-4000-8000-000000000004'::uuid, '{"name": "Delta Project", "status": "paused", "priority": "high"}'),
  ('a0000000-0000-4000-8000-000000000005'::uuid, '{"name": "Epsilon Project", "status": "active", "priority": "medium"}')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- Seed Data for user (Moderate JSON structure)
-- ============================================================================

INSERT INTO test.tb_user (id, data) VALUES
  ('b0000000-0000-4000-8000-000000000001'::uuid, '{
    "id": "user_1",
    "name": "Alice Johnson",
    "email": "alice@example.com",
    "roles": ["admin", "user"],
    "profile": {
      "avatar_url": "https://example.com/avatars/alice.jpg",
      "bio": "Software engineer with 10+ years experience",
      "location": "San Francisco, CA",
      "website": "https://alice.dev"
    },
    "settings": {
      "notifications": true,
      "theme": "dark",
      "language": "en"
    },
    "created_at": "2024-01-01T00:00:00Z"
  }'),
  ('b0000000-0000-4000-8000-000000000002'::uuid, '{
    "id": "user_2",
    "name": "Bob Smith",
    "email": "bob@example.com",
    "roles": ["user"],
    "profile": {
      "avatar_url": "https://example.com/avatars/bob.jpg",
      "bio": "Data scientist",
      "location": "New York, NY"
    },
    "settings": {
      "notifications": false,
      "theme": "light"
    },
    "created_at": "2024-01-02T00:00:00Z"
  }'),
  ('b0000000-0000-4000-8000-000000000003'::uuid, '{
    "id": "user_3",
    "name": "Carol White",
    "email": "carol@example.com",
    "roles": ["user", "moderator"],
    "profile": {
      "avatar_url": "https://example.com/avatars/carol.jpg",
      "bio": "Product manager",
      "location": "Austin, TX",
      "website": "https://carol.pm"
    },
    "settings": {
      "notifications": true,
      "theme": "dark",
      "language": "en",
      "privacy": "public"
    },
    "created_at": "2024-01-03T00:00:00Z"
  }')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- Seed Data for task (Complex nested structure)
-- ============================================================================

INSERT INTO test.tb_task (id, data) VALUES
  ('c0000000-0000-4000-8000-000000000001'::uuid, '{
    "id": "task_1",
    "title": "Implement API endpoint",
    "description": "Create REST API for user management",
    "status": "in_progress",
    "priority": "high",
    "assigned_to": "user_1",
    "project_id": "proj_1",
    "timeline": {
      "created_at": "2024-01-01T10:00:00Z",
      "started_at": "2024-01-02T09:30:00Z",
      "due_date": "2024-01-15T17:00:00Z",
      "estimated_hours": 16
    },
    "comments": [
      {
        "author": "user_2",
        "text": "This needs pagination support",
        "created_at": "2024-01-02T11:00:00Z",
        "likes": 3
      },
      {
        "author": "user_3",
        "text": "Let''s use standard REST conventions",
        "created_at": "2024-01-02T12:00:00Z",
        "likes": 5
      }
    ],
    "tags": ["backend", "api", "urgent"],
    "attachments": [
      {"name": "spec.pdf", "size": 2048, "url": "https://example.com/spec.pdf"},
      {"name": "wireframe.png", "size": 102400, "url": "https://example.com/wireframe.png"}
    ]
  }'),
  ('c0000000-0000-4000-8000-000000000002'::uuid, '{
    "id": "task_2",
    "title": "Write documentation",
    "description": "Document the new API endpoints with examples",
    "status": "todo",
    "priority": "medium",
    "assigned_to": "user_2",
    "project_id": "proj_1",
    "timeline": {
      "created_at": "2024-01-05T10:00:00Z",
      "due_date": "2024-01-20T17:00:00Z",
      "estimated_hours": 8
    },
    "dependencies": ["task_1"],
    "tags": ["documentation", "api"],
    "history": [
      {"action": "created", "by": "user_1", "at": "2024-01-05T10:00:00Z"},
      {"action": "assigned", "by": "user_1", "to": "user_2", "at": "2024-01-05T10:15:00Z"}
    ]
  }')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- Seed Data for document (Large JSON objects)
-- ============================================================================

INSERT INTO test.tb_document (id, data) VALUES
  ('d0000000-0000-4000-8000-000000000001'::uuid, '{
    "id": "doc_1",
    "title": "Quarterly Business Review",
    "content": "This is a comprehensive quarterly business review with detailed metrics and analysis across all departments.",
    "metadata": {
      "author": "user_1",
      "created_at": "2024-01-01T00:00:00Z",
      "updated_at": "2024-01-10T15:30:00Z",
      "version": 5,
      "status": "published"
    },
    "sections": [
      {
        "title": "Executive Summary",
        "content": "Overview of key metrics and achievements",
        "subsections": [
          {"title": "Revenue", "data": {"target": 1000000, "actual": 1050000, "variance": 5}},
          {"title": "Headcount", "data": {"target": 50, "actual": 48, "variance": -4}},
          {"title": "Customer Satisfaction", "data": {"target": 95, "actual": 96.5, "variance": 1.5}}
        ]
      },
      {
        "title": "Department Performance",
        "content": "Detailed breakdown by department",
        "departments": [
          {"name": "Engineering", "headcount": 15, "projects": 8, "on_time_delivery": 95},
          {"name": "Sales", "headcount": 12, "deals_closed": 45, "pipeline": 500000},
          {"name": "Marketing", "headcount": 8, "campaigns": 12, "roi": 3.5},
          {"name": "Operations", "headcount": 6, "tickets_resolved": 342, "satisfaction": 98}
        ]
      },
      {
        "title": "Financial Analysis",
        "content": "Comprehensive financial metrics",
        "metrics": {
          "revenue": {"previous_quarter": 950000, "current_quarter": 1050000, "growth": 10.5},
          "expenses": {"previous_quarter": 650000, "current_quarter": 680000, "growth": 4.6},
          "profit_margin": {"previous_quarter": 31.5, "current_quarter": 35.2}
        }
      }
    ],
    "attachments": [
      {"name": "revenue_chart.png", "size": 50000, "format": "image/png"},
      {"name": "forecast.xlsx", "size": 150000, "format": "application/vnd.ms-excel"},
      {"name": "detailed_metrics.csv", "size": 250000, "format": "text/csv"}
    ],
    "approvals": [
      {"approver": "user_1", "status": "approved", "at": "2024-01-10T10:00:00Z"},
      {"approver": "user_3", "status": "approved", "at": "2024-01-10T14:00:00Z"}
    ],
    "views": [
      {"user": "user_2", "viewed_at": "2024-01-10T15:00:00Z"},
      {"user": "user_4", "viewed_at": "2024-01-10T15:15:00Z"},
      {"user": "user_5", "viewed_at": "2024-01-10T15:30:00Z"}
    ]
  }')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- Verify seed data
-- ============================================================================

SELECT 'Seed data loaded successfully' as status,
       (SELECT COUNT(*) FROM test.tb_project) as project_count,
       (SELECT COUNT(*) FROM test.tb_user) as user_count,
       (SELECT COUNT(*) FROM test.tb_task) as task_count,
       (SELECT COUNT(*) FROM test.tb_document) as document_count;
