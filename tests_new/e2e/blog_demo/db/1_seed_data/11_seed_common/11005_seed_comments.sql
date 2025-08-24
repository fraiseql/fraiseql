-- Common seed data for comments
-- Always loaded for all environments

-- Create sample comments
INSERT INTO tb_comment (
    pk_comment,
    fk_post,
    fk_author,
    fk_parent_comment,
    content,
    status,
    moderation_data,
    created_at
) VALUES
-- Comments on GraphQL post
(
    'comm1111-1111-1111-1111-111111111111'::UUID,
    'post1111-1111-1111-1111-111111111111'::UUID,
    '44444444-4444-4444-4444-444444444444'::UUID, -- testuser
    NULL,
    'Great introduction to GraphQL! I''ve been wanting to learn this for a while. The examples are really clear and easy to follow.',
    'approved',
    jsonb_build_object(
        'approved_by', '55555555-5555-5555-5555-555555555555',
        'approved_at', '2024-01-15 12:00:00+00'
    ),
    '2024-01-15 11:30:00+00'
),
(
    'comm2222-2222-2222-2222-222222222222'::UUID,
    'post1111-1111-1111-1111-111111111111'::UUID,
    '22222222-2222-2222-2222-222222222222'::UUID, -- johndoe (author replying)
    'comm1111-1111-1111-1111-111111111111'::UUID,
    'Thanks! I''m glad you found it helpful. Let me know if you have any questions as you start implementing GraphQL.',
    'approved',
    jsonb_build_object(
        'auto_approved', true,
        'reason', 'author_reply'
    ),
    '2024-01-15 15:45:00+00'
),
(
    'comm3333-3333-3333-3333-333333333333'::UUID,
    'post1111-1111-1111-1111-111111111111'::UUID,
    '33333333-3333-3333-3333-333333333333'::UUID, -- janesmit
    NULL,
    'Excellent post! One thing to add is that GraphQL also provides better error handling compared to REST APIs. Looking forward to the next post in the series!',
    'approved',
    jsonb_build_object(
        'approved_by', '55555555-5555-5555-5555-555555555555',
        'approved_at', '2024-01-16 09:15:00+00'
    ),
    '2024-01-16 08:20:00+00'
),

-- Comments on PostgreSQL post
(
    'comm4444-4444-4444-4444-444444444444'::UUID,
    'post2222-2222-2222-2222-222222222222'::UUID,
    '22222222-2222-2222-2222-222222222222'::UUID, -- johndoe
    NULL,
    'This is incredibly detailed! I never knew about partial indexes. The performance tips section is gold. Thanks for sharing your expertise, Jane!',
    'approved',
    jsonb_build_object(
        'approved_by', '55555555-5555-5555-5555-555555555555',
        'approved_at', '2024-01-20 16:00:00+00'
    ),
    '2024-01-20 15:30:00+00'
),
(
    'comm5555-5555-5555-5555-555555555555'::UUID,
    'post2222-2222-2222-2222-222222222222'::UUID,
    '44444444-4444-4444-4444-444444444444'::UUID, -- testuser
    NULL,
    'Quick question: when should I use GIN vs GiST indexes for JSONB data?',
    'approved',
    jsonb_build_object(
        'approved_by', '55555555-5555-5555-5555-555555555555',
        'approved_at', '2024-01-21 10:30:00+00'
    ),
    '2024-01-21 09:45:00+00'
),
(
    'comm6666-6666-6666-6666-666666666666'::UUID,
    'post2222-2222-2222-2222-222222222222'::UUID,
    '33333333-3333-3333-3333-333333333333'::UUID, -- janesmit (author replying)
    'comm5555-5555-5555-5555-555555555555'::UUID,
    'Great question! For JSONB, GIN is usually the better choice for most use cases. GIN is optimized for containment queries (@>, ?, ?&, ?|) which are the most common JSONB operations. GiST can be useful for more complex geometric or range operations, but for general JSONB indexing, stick with GIN.',
    'approved',
    jsonb_build_object(
        'auto_approved', true,
        'reason', 'author_reply'
    ),
    '2024-01-21 14:20:00+00'
),

-- Pending comment (for moderation testing)
(
    'comm7777-7777-7777-7777-777777777777'::UUID,
    'post1111-1111-1111-1111-111111111111'::UUID,
    '44444444-4444-4444-4444-444444444444'::UUID, -- testuser
    NULL,
    'This comment is still pending moderation. It should not appear in public queries.',
    'pending',
    jsonb_build_object(),
    '2024-01-25 12:00:00+00'
);

-- Set the sequence to avoid conflicts
SELECT setval('tb_comment_id_seq', 1000, true);
