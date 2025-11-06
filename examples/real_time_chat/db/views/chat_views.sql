-- Chat Views for Real-time Chat API
-- Optimized for GraphQL queries with FraiseQL

-- Users view
CREATE OR REPLACE VIEW v_user AS
SELECT
    pk_user,
    id,
    jsonb_build_object(
        'id', id,
        'username', username,
        'email', email,
        'display_name', display_name,
        'avatar_url', avatar_url,
        'status', status,
        'last_seen', last_seen,
        'is_active', is_active,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM tb_user;

-- Rooms view
CREATE OR REPLACE VIEW v_room AS
SELECT
    pk_room,
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'slug', slug,
        'description', description,
        'type', type,
        'fk_owner', fk_owner,
        'max_members', max_members,
        'is_active', is_active,
        'settings', settings,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM tb_room;

-- Room list view with latest message and unread count
CREATE OR REPLACE VIEW v_room_list AS
SELECT
    r.pk_room,
    r.id,
    jsonb_build_object(
        'id', r.id,
        'name', r.name,
        'slug', r.slug,
        'description', r.description,
        'type', r.type,
        'fk_owner', r.fk_owner,
        'max_members', r.max_members,
        'is_active', r.is_active,
        'settings', r.settings,
        'created_at', r.created_at,
        'updated_at', r.updated_at,
        'owner', u.data,
        'member_count', COUNT(DISTINCT rm.fk_user) FILTER (WHERE rm.is_banned = false),
        'online_count', COUNT(DISTINCT up.fk_user) FILTER (WHERE up.status = 'online'),
        'latest_message', (
            SELECT json_build_object(
                'id', m.id,
                'content', m.content,
                'message_type', m.message_type,
                'created_at', m.created_at,
                'fk_user', m.fk_user
            )
            FROM messages m
            WHERE m.fk_room = r.pk_room
            AND m.is_deleted = false
            ORDER BY m.created_at DESC
            LIMIT 1
        )
    ) as data
FROM tb_room r
JOIN v_user u ON r.fk_owner = u.pk_user
LEFT JOIN tb_room_member rm ON rm.fk_room = r.pk_room AND rm.is_banned = false
LEFT JOIN tb_user_presence up ON up.fk_user = rm.fk_user AND up.fk_room = r.pk_room
WHERE r.is_active = true
GROUP BY r.pk_room, r.id, r.name, r.slug, r.description, r.type, r.fk_owner, r.max_members, r.is_active, r.settings, r.created_at, r.updated_at, u.data;

-- Room detail view with members and permissions
CREATE OR REPLACE VIEW v_room_detail AS
SELECT
    r.id,
    r.name,
    r.slug,
    r.description,
    r.type,
    r.fk_owner,
    r.max_members,
    r.is_active,
    r.settings,
    r.created_at,
    r.updated_at,
    -- Owner info
    json_build_object(
        'id', owner.id,
        'username', owner.username,
        'display_name', owner.display_name,
        'avatar_url', owner.avatar_url,
        'status', owner.status
    ) as owner,
    -- Members with roles
    COALESCE(
        json_agg(DISTINCT
            json_build_object(
                'id', rm.id,
                'fk_user', rm.fk_user,
                'role', rm.role,
                'joined_at', rm.joined_at,
                'last_read_at', rm.last_read_at,
                'is_muted', rm.is_muted,
                'user', json_build_object(
                    'id', u.id,
                    'username', u.username,
                    'display_name', u.display_name,
                    'avatar_url', u.avatar_url,
                    'status', u.status,
                    'last_seen', u.last_seen
                )
            )
        ) FILTER (WHERE rm.id IS NOT NULL AND rm.is_banned = false),
        '[]'::json
    ) as members,
    -- Statistics
    COUNT(DISTINCT rm.user_id) FILTER (WHERE rm.is_banned = false) as member_count,
    COUNT(DISTINCT m.id) FILTER (WHERE m.is_deleted = false) as message_count,
    COUNT(DISTINCT up.user_id) FILTER (WHERE up.status = 'online') as online_count
FROM tb_room r
JOIN tb_user owner ON r.fk_owner = owner.pk_user
LEFT JOIN tb_room_member rm ON rm.fk_room = r.pk_room AND rm.is_banned = false
LEFT JOIN tb_user u ON rm.fk_user = u.pk_user
LEFT JOIN tb_message m ON m.fk_room = r.pk_room
LEFT JOIN user_presence up ON up.fk_user = rm.fk_user AND up.fk_room = r.pk_room
GROUP BY r.id, owner.id, owner.username, owner.display_name, owner.avatar_url, owner.status;

-- Message thread view with reactions and replies
CREATE OR REPLACE VIEW v_message_thread AS
SELECT
    m.pk_message,
    m.id,
    jsonb_build_object(
        'id', m.id,
        'fk_room', m.fk_room,
        'fk_user', m.fk_user,
        'content', m.content,
        'message_type', m.message_type,
        'fk_parent_message', m.fk_parent_message,
        'edited_at', m.edited_at,
        'is_deleted', m.is_deleted,
        'metadata', m.metadata,
        'created_at', m.created_at,
        'author', json_build_object(
            'id', u.id,
            'username', u.username,
            'display_name', u.display_name,
            'avatar_url', u.avatar_url,
            'status', u.status
        ),
        'attachments', COALESCE(
            json_agg(DISTINCT
                json_build_object(
                    'id', ma.id,
                    'filename', ma.filename,
                    'original_filename', ma.original_filename,
                    'file_size', ma.file_size,
                    'mime_type', ma.mime_type,
                    'url', ma.url,
                    'thumbnail_url', ma.thumbnail_url,
                    'width', ma.width,
                    'height', ma.height,
                    'duration', ma.duration
                )
            ) FILTER (WHERE ma.id IS NOT NULL),
            '[]'::json
        ),
        'reactions', COALESCE(
            json_agg(DISTINCT
                json_build_object(
                    'emoji', mr.emoji,
                    'count', COUNT(*) OVER (PARTITION BY mr.emoji),
                    'users', json_agg(
                        json_build_object(
                            'id', ru.id,
                            'username', ru.username,
                            'display_name', ru.display_name
                        )
                    ) OVER (PARTITION BY mr.emoji)
                )
            ) FILTER (WHERE mr.id IS NOT NULL),
            '[]'::json
        ),
        'reply_count', COUNT(DISTINCT replies.id),
        'read_count', COUNT(DISTINCT mrr.fk_user)
    ) as data
FROM tb_message m
JOIN tb_user u ON m.fk_user = u.pk_user
LEFT JOIN tb_message_attachment ma ON ma.fk_message = m.pk_message
LEFT JOIN tb_message_reaction mr ON mr.fk_message = m.pk_message
LEFT JOIN tb_user ru ON mr.fk_user = ru.pk_user
LEFT JOIN tb_message replies ON replies.fk_parent_message = m.pk_message AND replies.is_deleted = false
LEFT JOIN tb_message_read_receipt mrr ON mrr.fk_message = m.pk_message
WHERE m.is_deleted = false
GROUP BY m.pk_message, m.id, m.fk_room, m.fk_user, m.content, m.message_type, m.fk_parent_message, m.edited_at, m.is_deleted, m.metadata, m.created_at, u.id, u.username, u.display_name, u.avatar_url, u.status;

-- User conversation view (DMs and room memberships)
CREATE OR REPLACE VIEW v_user_conversation AS
SELECT
    rm.fk_user,
    r.id,
    r.name,
    r.slug,
    r.type,
    r.description,
    rm.role,
    rm.joined_at,
    rm.last_read_at,
    rm.is_muted,
    -- Unread message count
    COUNT(m.id) FILTER (WHERE m.created_at > rm.last_read_at AND m.fk_user != rm.fk_user) as unread_count,
    -- Latest message
    (
        SELECT json_build_object(
            'id', latest.id,
            'content', latest.content,
            'message_type', latest.message_type,
            'created_at', latest.created_at,
            'author', json_build_object(
                'username', latest_user.username,
                'display_name', latest_user.display_name
            )
        )
        FROM messages latest
        JOIN tb_user latest_user ON latest.fk_user = latest_user.pk_user
        WHERE latest.fk_room = r.pk_room
        AND latest.is_deleted = false
        ORDER BY latest.created_at DESC
        LIMIT 1
    ) as latest_message,
    -- For direct conversations, get the other user
    CASE
        WHEN r.type = 'direct' THEN
            (
                SELECT json_build_object(
                    'id', other_user.id,
                    'username', other_user.username,
                    'display_name', other_user.display_name,
                    'avatar_url', other_user.avatar_url,
                    'status', other_user.status
                )
                 FROM tb_room_member other_rm
                 JOIN tb_user other_user ON other_rm.fk_user = other_user.pk_user
                 WHERE other_rm.fk_room = r.pk_room
                 AND other_rm.fk_user != rm.fk_user
                 LIMIT 1
            )
        ELSE NULL
    END as direct_user
FROM tb_room_member rm
JOIN tb_room r ON rm.fk_room = r.pk_room
LEFT JOIN tb_message m ON m.fk_room = r.pk_room AND m.is_deleted = false
WHERE rm.is_banned = false
  AND r.is_active = true
GROUP BY rm.user_id, r.id, rm.role, rm.joined_at, rm.last_read_at, rm.is_muted;

-- Online users view
CREATE OR REPLACE VIEW v_online_user AS
SELECT DISTINCT
    u.id,
    u.username,
    u.display_name,
    u.avatar_url,
    u.status,
    u.last_seen,
    -- Rooms where user is online
    COALESCE(
        json_agg(DISTINCT
            json_build_object(
                'fk_room', up.fk_room,
                'last_activity', up.last_activity
            )
        ) FILTER (WHERE up.fk_room IS NOT NULL),
        '[]'::json
    ) as active_rooms
FROM tb_user u
JOIN tb_user_presence up ON up.fk_user = u.pk_user
WHERE up.status = 'online'
  AND up.last_activity > CURRENT_TIMESTAMP - INTERVAL '5 minutes'
  AND u.is_active = true
GROUP BY u.id;

-- Typing indicators view
CREATE OR REPLACE VIEW v_active_typing AS
SELECT
    ti.fk_room,
    json_agg(
        json_build_object(
            'fk_user', ti.fk_user,
            'username', u.username,
            'display_name', u.display_name,
            'started_at', ti.started_at,
            'expires_at', ti.expires_at
        )
    ) as typing_users
FROM tb_typing_indicator ti
JOIN tb_user u ON ti.fk_user = u.pk_user
WHERE ti.expires_at > CURRENT_TIMESTAMP
GROUP BY ti.fk_room;

-- Message search view
CREATE OR REPLACE VIEW v_message_search AS
SELECT
    m.id,
    m.fk_room,
    m.fk_user,
    m.content,
    m.message_type,
    m.created_at,
    -- Room info
    json_build_object(
        'id', r.id,
        'name', r.name,
        'type', r.type
    ) as room,
    -- Author info
    json_build_object(
        'id', u.id,
        'username', u.username,
        'display_name', u.display_name,
        'avatar_url', u.avatar_url
    ) as author,
    -- Search vector
    to_tsvector('english', m.content) as search_vector,
    -- Search rank (for relevance scoring)
    ts_rank(to_tsvector('english', m.content), plainto_tsquery('english', '')) as search_rank
FROM tb_message m
JOIN tb_room r ON m.fk_room = r.pk_room
JOIN tb_user u ON m.fk_user = u.pk_user
WHERE m.is_deleted = false
  AND r.is_active = true;

-- Room analytics view
CREATE OR REPLACE VIEW v_room_analytic AS
SELECT
    r.id,
    r.name,
    r.type,
    DATE_TRUNC('day', r.created_at) as created_date,
    -- Message statistics
    COUNT(DISTINCT m.id) as total_messages,
    COUNT(DISTINCT m.id) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '7 days') as messages_last_7_days,
    COUNT(DISTINCT m.id) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '30 days') as messages_last_30_days,
    -- User statistics
    COUNT(DISTINCT rm.fk_user) FILTER (WHERE rm.is_banned = false) as total_members,
    COUNT(DISTINCT m.fk_user) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '7 days') as active_users_7_days,
    COUNT(DISTINCT m.fk_user) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '30 days') as active_users_30_days,
    -- Activity patterns
    AVG(daily_stats.message_count) as avg_daily_messages,
    MAX(daily_stats.message_count) as peak_daily_messages
FROM tb_room r
LEFT JOIN tb_room_member rm ON rm.fk_room = r.pk_room
LEFT JOIN tb_message m ON m.fk_room = r.pk_room AND m.is_deleted = false
LEFT JOIN LATERAL (
    SELECT
        DATE_TRUNC('day', created_at) as day,
        COUNT(*) as message_count
    FROM messages
    WHERE fk_room = r.pk_room
    AND is_deleted = false
    AND created_at >= CURRENT_DATE - INTERVAL '30 days'
    GROUP BY DATE_TRUNC('day', created_at)
) daily_stats ON true
WHERE r.is_active = true
GROUP BY r.id, r.name, r.type, r.created_at;
