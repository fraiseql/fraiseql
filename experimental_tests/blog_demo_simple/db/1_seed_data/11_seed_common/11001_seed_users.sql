-- Common seed data for users
-- Always loaded for all environments

-- Create test users for E2E testing
INSERT INTO tb_user (
    pk_user,
    identifier,
    email,
    password_hash,
    role,
    is_active,
    email_verified,
    profile,
    created_at
) VALUES
(
    '11111111-1111-1111-1111-111111111111'::UUID,
    'admin',
    'admin@blog.demo',
    '$2b$12$dummy_hash_for_admin_password',
    'admin',
    true,
    true,
    jsonb_build_object(
        'firstName', 'Admin',
        'lastName', 'User',
        'bio', 'System Administrator',
        'website', 'https://blog.demo'
    ),
    '2024-01-01 00:00:00+00'
),
(
    '22222222-2222-2222-2222-222222222222'::UUID,
    'johndoe',
    'john.doe@example.com',
    '$2b$12$dummy_hash_for_john_password',
    'author',
    true,
    true,
    jsonb_build_object(
        'firstName', 'John',
        'lastName', 'Doe',
        'bio', 'Tech blogger and software developer',
        'website', 'https://johndoe.dev',
        'avatar_url', 'https://gravatar.com/avatar/johndoe'
    ),
    '2024-01-01 01:00:00+00'
),
(
    '33333333-3333-3333-3333-333333333333'::UUID,
    'janesmit',
    'jane.smith@example.com',
    '$2b$12$dummy_hash_for_jane_password',
    'author',
    true,
    true,
    jsonb_build_object(
        'firstName', 'Jane',
        'lastName', 'Smith',
        'bio', 'Frontend developer and UI/UX designer',
        'website', 'https://janesmith.design'
    ),
    '2024-01-01 02:00:00+00'
),
(
    '44444444-4444-4444-4444-444444444444'::UUID,
    'testuser',
    'test.user@example.com',
    '$2b$12$dummy_hash_for_test_password',
    'user',
    true,
    false,
    jsonb_build_object(
        'firstName', 'Test',
        'lastName', 'User',
        'bio', 'Just a regular user for testing'
    ),
    '2024-01-01 03:00:00+00'
),
(
    '55555555-5555-5555-5555-555555555555'::UUID,
    'moderator',
    'mod@blog.demo',
    '$2b$12$dummy_hash_for_mod_password',
    'moderator',
    true,
    true,
    jsonb_build_object(
        'firstName', 'Blog',
        'lastName', 'Moderator',
        'bio', 'Content moderator'
    ),
    '2024-01-01 04:00:00+00'
);

-- Set the sequence to avoid conflicts
SELECT setval('tb_user_id_seq', 1000, true);
