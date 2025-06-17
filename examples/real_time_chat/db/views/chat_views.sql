-- Chat Views for Real-time Chat API
-- Optimized for GraphQL queries with FraiseQL

-- Room list view with latest message and unread count
CREATE OR REPLACE VIEW room_list AS
SELECT
    r.id,
    r.name,
    r.slug,
    r.description,
    r.type,
    r.owner_id,
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
        'avatar_url', owner.avatar_url
    ) as owner,
    -- Member count
    COUNT(DISTINCT rm.user_id) FILTER (WHERE rm.is_banned = false) as member_count,
    -- Online member count
    COUNT(DISTINCT up.user_id) FILTER (WHERE up.status = 'online') as online_count,
    -- Latest message
    (
        SELECT json_build_object(
            'id', m.id,
            'content', m.content,
            'message_type', m.message_type,
            'created_at', m.created_at,
            'user', json_build_object(
                'id', u.id,
                'username', u.username,
                'display_name', u.display_name,
                'avatar_url', u.avatar_url
            )
        )
        FROM messages m
        JOIN users u ON m.user_id = u.id
        WHERE m.room_id = r.id
        AND m.is_deleted = false
        ORDER BY m.created_at DESC
        LIMIT 1
    ) as latest_message
FROM rooms r
JOIN users owner ON r.owner_id = owner.id
LEFT JOIN room_members rm ON rm.room_id = r.id AND rm.is_banned = false
LEFT JOIN user_presence up ON up.user_id = rm.user_id AND up.room_id = r.id
WHERE r.is_active = true
GROUP BY r.id, owner.id, owner.username, owner.display_name, owner.avatar_url;

-- Room detail view with members and permissions
CREATE OR REPLACE VIEW room_detail AS
SELECT
    r.*,
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
                'user_id', rm.user_id,
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
FROM rooms r
JOIN users owner ON r.owner_id = owner.id
LEFT JOIN room_members rm ON rm.room_id = r.id AND rm.is_banned = false
LEFT JOIN users u ON rm.user_id = u.id
LEFT JOIN messages m ON m.room_id = r.id
LEFT JOIN user_presence up ON up.user_id = rm.user_id AND up.room_id = r.id
GROUP BY r.id, owner.id, owner.username, owner.display_name, owner.avatar_url, owner.status;

-- Message thread view with reactions and replies
CREATE OR REPLACE VIEW message_thread AS
SELECT
    m.id,
    m.room_id,
    m.user_id,
    m.content,
    m.message_type,
    m.parent_message_id,
    m.edited_at,
    m.is_deleted,
    m.metadata,
    m.created_at,
    -- Author info
    json_build_object(
        'id', u.id,
        'username', u.username,
        'display_name', u.display_name,
        'avatar_url', u.avatar_url,
        'status', u.status
    ) as author,
    -- Attachments
    COALESCE(
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
    ) as attachments,
    -- Reactions
    COALESCE(
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
    ) as reactions,
    -- Reply count
    COUNT(DISTINCT replies.id) as reply_count,
    -- Read receipts
    COUNT(DISTINCT mrr.user_id) as read_count
FROM messages m
JOIN users u ON m.user_id = u.id
LEFT JOIN message_attachments ma ON ma.message_id = m.id
LEFT JOIN message_reactions mr ON mr.message_id = m.id
LEFT JOIN users ru ON mr.user_id = ru.id
LEFT JOIN messages replies ON replies.parent_message_id = m.id AND replies.is_deleted = false
LEFT JOIN message_read_receipts mrr ON mrr.message_id = m.id
WHERE m.is_deleted = false
GROUP BY m.id, u.id, u.username, u.display_name, u.avatar_url, u.status;

-- User conversation view (DMs and room memberships)
CREATE OR REPLACE VIEW user_conversations AS
SELECT
    rm.user_id,
    r.id as room_id,
    r.name,
    r.slug,
    r.type,
    r.description,
    rm.role,
    rm.joined_at,
    rm.last_read_at,
    rm.is_muted,
    -- Unread message count
    COUNT(m.id) FILTER (WHERE m.created_at > rm.last_read_at AND m.user_id != rm.user_id) as unread_count,
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
        JOIN users latest_user ON latest.user_id = latest_user.id
        WHERE latest.room_id = r.id
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
                FROM room_members other_rm
                JOIN users other_user ON other_rm.user_id = other_user.id
                WHERE other_rm.room_id = r.id
                AND other_rm.user_id != rm.user_id
                LIMIT 1
            )
        ELSE NULL
    END as direct_user
FROM room_members rm
JOIN rooms r ON rm.room_id = r.id
LEFT JOIN messages m ON m.room_id = r.id AND m.is_deleted = false
WHERE rm.is_banned = false
  AND r.is_active = true
GROUP BY rm.user_id, r.id, rm.role, rm.joined_at, rm.last_read_at, rm.is_muted;

-- Online users view
CREATE OR REPLACE VIEW online_users AS
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
                'room_id', up.room_id,
                'last_activity', up.last_activity
            )
        ) FILTER (WHERE up.room_id IS NOT NULL),
        '[]'::json
    ) as active_rooms
FROM users u
JOIN user_presence up ON up.user_id = u.id
WHERE up.status = 'online'
  AND up.last_activity > CURRENT_TIMESTAMP - INTERVAL '5 minutes'
  AND u.is_active = true
GROUP BY u.id;

-- Typing indicators view
CREATE OR REPLACE VIEW active_typing AS
SELECT
    ti.room_id,
    json_agg(
        json_build_object(
            'user_id', u.id,
            'username', u.username,
            'display_name', u.display_name,
            'started_at', ti.started_at,
            'expires_at', ti.expires_at
        )
    ) as typing_users
FROM typing_indicators ti
JOIN users u ON ti.user_id = u.id
WHERE ti.expires_at > CURRENT_TIMESTAMP
GROUP BY ti.room_id;

-- Message search view
CREATE OR REPLACE VIEW message_search AS
SELECT
    m.id,
    m.room_id,
    m.user_id,
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
FROM messages m
JOIN rooms r ON m.room_id = r.id
JOIN users u ON m.user_id = u.id
WHERE m.is_deleted = false
  AND r.is_active = true;

-- Room analytics view
CREATE OR REPLACE VIEW room_analytics AS
SELECT
    r.id as room_id,
    r.name,
    r.type,
    DATE_TRUNC('day', r.created_at) as created_date,
    -- Message statistics
    COUNT(DISTINCT m.id) as total_messages,
    COUNT(DISTINCT m.id) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '7 days') as messages_last_7_days,
    COUNT(DISTINCT m.id) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '30 days') as messages_last_30_days,
    -- User statistics
    COUNT(DISTINCT rm.user_id) FILTER (WHERE rm.is_banned = false) as total_members,
    COUNT(DISTINCT m.user_id) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '7 days') as active_users_7_days,
    COUNT(DISTINCT m.user_id) FILTER (WHERE m.created_at >= CURRENT_DATE - INTERVAL '30 days') as active_users_30_days,
    -- Activity patterns
    AVG(daily_stats.message_count) as avg_daily_messages,
    MAX(daily_stats.message_count) as peak_daily_messages
FROM rooms r
LEFT JOIN room_members rm ON rm.room_id = r.id
LEFT JOIN messages m ON m.room_id = r.id AND m.is_deleted = false
LEFT JOIN LATERAL (
    SELECT
        DATE_TRUNC('day', created_at) as day,
        COUNT(*) as message_count
    FROM messages
    WHERE room_id = r.id
    AND is_deleted = false
    AND created_at >= CURRENT_DATE - INTERVAL '30 days'
    GROUP BY DATE_TRUNC('day', created_at)
) daily_stats ON true
WHERE r.is_active = true
GROUP BY r.id, r.name, r.type, r.created_at;
