-- fraiseql-wire Test Data Seed Script
--
-- This script populates the staging database with realistic test data.
-- Includes various JSON shapes: small, medium, large, and deeply nested.
--
-- Run with: psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql

-- Ensure schema exists
CREATE SCHEMA IF NOT EXISTS test_staging;

-- ============================================================================
-- Seed Data for Projects (Simple JSON structure)
-- ============================================================================

-- Small projects (< 1KB each)
INSERT INTO test_staging.projects (id, data) VALUES
  (gen_random_uuid(), '{"name": "Alpha Project", "status": "active", "priority": "high"}'),
  (gen_random_uuid(), '{"name": "Beta Project", "status": "archived", "priority": "low"}'),
  (gen_random_uuid(), '{"name": "Gamma Project", "status": "active", "priority": "medium"}'),
  (gen_random_uuid(), '{"name": "Delta Project", "status": "paused", "priority": "high"}'),
  (gen_random_uuid(), '{"name": "Epsilon Project", "status": "active", "priority": "medium"}')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Seed Data for Users (Moderate JSON structure)
-- ============================================================================

-- Users with nested information (2-5KB each)
INSERT INTO test_staging.users (id, data) VALUES
  (gen_random_uuid(), '{
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
  (gen_random_uuid(), '{
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
  (gen_random_uuid(), '{
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
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Seed Data for Tasks (Complex nested structure)
-- ============================================================================

-- Tasks with deeply nested metadata (5-20KB each)
INSERT INTO test_staging.tasks (id, data) VALUES
  (gen_random_uuid(), '{
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
  (gen_random_uuid(), '{
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
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Seed Data for Documents (Large JSON objects)
-- ============================================================================

-- Large documents with extensive nested structures (100KB+ each)
-- For memory stress testing
INSERT INTO test_staging.documents (id, data) VALUES
  (gen_random_uuid(), '{
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
ON CONFLICT DO NOTHING;

-- ============================================================================
-- Verify seed data
-- ============================================================================

-- Show summary of inserted data
SELECT 'Seed data loaded successfully' as status,
       (SELECT COUNT(*) FROM test_staging.projects) as projects_count,
       (SELECT COUNT(*) FROM test_staging.users) as users_count,
       (SELECT COUNT(*) FROM test_staging.tasks) as tasks_count,
       (SELECT COUNT(*) FROM test_staging.documents) as documents_count;
