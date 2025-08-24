-- Common seed data for post-tag relationships
-- Always loaded for all environments

-- Associate posts with tags
INSERT INTO tb_post_tag (fk_post, fk_tag, created_at) VALUES
-- GraphQL post tags
('post1111-1111-1111-1111-111111111111'::UUID, 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'::UUID, '2024-01-15 10:00:00+00'), -- GraphQL
('post1111-1111-1111-1111-111111111111'::UUID, 'cccccccc-cccc-cccc-cccc-cccccccccccc'::UUID, '2024-01-15 10:00:00+00'), -- Web Development
('post1111-1111-1111-1111-111111111111'::UUID, 'ffffffff-ffff-ffff-ffff-ffffffffffff'::UUID, '2024-01-15 10:00:00+00'), -- Tutorial

-- PostgreSQL post tags
('post2222-2222-2222-2222-222222222222'::UUID, 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb'::UUID, '2024-01-20 14:30:00+00'), -- PostgreSQL
('post2222-2222-2222-2222-222222222222'::UUID, 'ffffffff-ffff-ffff-ffff-ffffffffffff'::UUID, '2024-01-20 14:30:00+00'), -- Tutorial

-- Draft post tags (for testing)
('post3333-3333-3333-3333-333333333333'::UUID, 'cccccccc-cccc-cccc-cccc-cccccccccccc'::UUID, '2024-01-25 16:00:00+00'), -- Web Development
('post3333-3333-3333-3333-333333333333'::UUID, 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee'::UUID, '2024-01-25 16:00:00+00'); -- JavaScript
