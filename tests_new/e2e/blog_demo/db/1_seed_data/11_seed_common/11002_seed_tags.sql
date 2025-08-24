-- Common seed data for tags
-- Always loaded for all environments

-- Create basic tags for categorization
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
    'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'::UUID,
    'graphql',
    'GraphQL',
    'GraphQL related posts and tutorials',
    '#E10098',
    1,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb'::UUID,
    'postgresql',
    'PostgreSQL',
    'PostgreSQL database tutorials and tips',
    '#336791',
    2,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'cccccccc-cccc-cccc-cccc-cccccccccccc'::UUID,
    'web-development',
    'Web Development',
    'General web development topics',
    '#61DAFB',
    3,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'dddddddd-dddd-dddd-dddd-dddddddddddd'::UUID,
    'python',
    'Python',
    'Python programming language',
    '#3776AB',
    4,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee'::UUID,
    'javascript',
    'JavaScript',
    'JavaScript and related frameworks',
    '#F7DF1E',
    5,
    true,
    '2024-01-01 00:00:00+00'
),
(
    'ffffffff-ffff-ffff-ffff-ffffffffffff'::UUID,
    'tutorial',
    'Tutorial',
    'Step-by-step guides and tutorials',
    '#28A745',
    6,
    true,
    '2024-01-01 00:00:00+00'
),
(
    '10101010-1010-1010-1010-101010101010'::UUID,
    'archived-tag',
    'Archived Tag',
    'This tag is not active',
    '#6C757D',
    99,
    false,
    '2024-01-01 00:00:00+00'
);

-- Set the sequence to avoid conflicts
SELECT setval('tb_tag_id_seq', 1000, true);
