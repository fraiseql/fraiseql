-- Seed data specific to E2E workflow tests
-- Used by test_user_registration_to_first_post_workflow

-- This file can be loaded before specific E2E tests that need additional data
-- beyond the common seed data

-- Add a test user that will be created during the E2E workflow
-- (This demonstrates how test-specific data can be prepared)

-- Example: Pre-create some content that the test user will interact with
INSERT INTO tb_post (
    pk_post,
    identifier,
    fk_author,
    title,
    content,
    excerpt,
    status,
    featured,
    published_at,
    created_at
) VALUES
(
    'e2e-test-post-1111-1111-111111111111'::UUID,
    'e2e-test-existing-post',
    '22222222-2222-2222-2222-222222222222'::UUID, -- johndoe
    'E2E Test - Existing Post',
    'This is an existing post that can be used for E2E testing interactions like commenting, liking, or referencing.',
    'A post that exists before the E2E test runs.',
    'published',
    false,
    '2024-01-01 12:00:00+00',
    '2024-01-01 12:00:00+00'
);

-- Add a test tag that can be used in E2E workflows
INSERT INTO tb_tag (
    pk_tag,
    identifier,
    name,
    description,
    color,
    sort_order,
    is_active,
    created_at
) VALUES
(
    'e2e-test-tag-1111-1111-111111111111'::UUID,
    'e2e-testing',
    'E2E Testing',
    'Tag for content created during E2E tests',
    '#FF6B35',
    100,
    true,
    '2024-01-01 12:00:00+00'
);

-- Note: This approach allows us to:
-- 1. Have consistent baseline data (common seeds)
-- 2. Add test-specific data that won't conflict with other tests
-- 3. Use predictable UUIDs for test assertions
-- 4. Clean up test data if needed (by UUID pattern)
