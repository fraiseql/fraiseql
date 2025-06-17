-- Chat Functions for Real-time Chat API
-- CQRS pattern: Functions for mutations

-- Create a new room
CREATE OR REPLACE FUNCTION create_room(
    p_name VARCHAR,
    p_slug VARCHAR,
    p_description TEXT,
    p_type VARCHAR,
    p_owner_id UUID,
    p_max_members INTEGER DEFAULT 1000,
    p_settings JSONB DEFAULT '{}'
) RETURNS JSON AS $$
DECLARE
    v_room_id UUID;
BEGIN
    -- Check if slug is available
    IF EXISTS (SELECT 1 FROM rooms WHERE slug = p_slug) THEN
        RAISE EXCEPTION 'Room slug already exists';
    END IF;

    -- Validate room type
    IF p_type NOT IN ('public', 'private', 'direct') THEN
        RAISE EXCEPTION 'Invalid room type';
    END IF;

    -- Create room
    INSERT INTO rooms (name, slug, description, type, owner_id, max_members, settings)
    VALUES (p_name, p_slug, p_description, p_type, p_owner_id, p_max_members, p_settings)
    RETURNING id INTO v_room_id;

    -- Add owner as admin member
    INSERT INTO room_members (room_id, user_id, role)
    VALUES (v_room_id, p_owner_id, 'owner');

    RETURN json_build_object(
        'success', true,
        'room_id', v_room_id,
        'message', 'Room created successfully'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Join a room
CREATE OR REPLACE FUNCTION join_room(
    p_room_id UUID,
    p_user_id UUID,
    p_role VARCHAR DEFAULT 'member'
) RETURNS JSON AS $$
DECLARE
    v_room RECORD;
    v_member_count INTEGER;
BEGIN
    -- Get room info
    SELECT * INTO v_room FROM rooms WHERE id = p_room_id AND is_active = true;

    IF v_room IS NULL THEN
        RAISE EXCEPTION 'Room not found or inactive';
    END IF;

    -- Check if already a member
    IF EXISTS (
        SELECT 1 FROM room_members
        WHERE room_id = p_room_id AND user_id = p_user_id
    ) THEN
        RAISE EXCEPTION 'User is already a member';
    END IF;

    -- Check room capacity
    SELECT COUNT(*) INTO v_member_count
    FROM room_members
    WHERE room_id = p_room_id AND is_banned = false;

    IF v_member_count >= v_room.max_members THEN
        RAISE EXCEPTION 'Room is at maximum capacity';
    END IF;

    -- For private rooms, check if user has permission (simplified)
    IF v_room.type = 'private' THEN
        -- In a real implementation, you'd check invitations or permissions
        NULL;
    END IF;

    -- Add user to room
    INSERT INTO room_members (room_id, user_id, role)
    VALUES (p_room_id, p_user_id, p_role);

    -- Create system message
    INSERT INTO messages (room_id, user_id, content, message_type, metadata)
    VALUES (
        p_room_id,
        p_user_id,
        'joined the room',
        'system',
        json_build_object('action', 'user_joined')::jsonb
    );

    RETURN json_build_object(
        'success', true,
        'message', 'Successfully joined room'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Send a message
CREATE OR REPLACE FUNCTION send_message(
    p_room_id UUID,
    p_user_id UUID,
    p_content TEXT,
    p_message_type VARCHAR DEFAULT 'text',
    p_parent_message_id UUID DEFAULT NULL,
    p_metadata JSONB DEFAULT '{}'
) RETURNS JSON AS $$
DECLARE
    v_message_id UUID;
    v_room_member RECORD;
BEGIN
    -- Check if user is a member of the room
    SELECT * INTO v_room_member
    FROM room_members
    WHERE room_id = p_room_id AND user_id = p_user_id AND is_banned = false;

    IF v_room_member IS NULL THEN
        RAISE EXCEPTION 'User is not a member of this room or is banned';
    END IF;

    -- Validate message type
    IF p_message_type NOT IN ('text', 'image', 'file', 'system') THEN
        RAISE EXCEPTION 'Invalid message type';
    END IF;

    -- Insert message
    INSERT INTO messages (room_id, user_id, content, message_type, parent_message_id, metadata)
    VALUES (p_room_id, p_user_id, p_content, p_message_type, p_parent_message_id, p_metadata)
    RETURNING id INTO v_message_id;

    -- Update last read timestamp for sender
    UPDATE room_members
    SET last_read_at = CURRENT_TIMESTAMP
    WHERE room_id = p_room_id AND user_id = p_user_id;

    -- Clear any typing indicator for this user
    DELETE FROM typing_indicators
    WHERE room_id = p_room_id AND user_id = p_user_id;

    RETURN json_build_object(
        'success', true,
        'message_id', v_message_id,
        'message', 'Message sent successfully'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Edit a message
CREATE OR REPLACE FUNCTION edit_message(
    p_message_id UUID,
    p_user_id UUID,
    p_new_content TEXT
) RETURNS JSON AS $$
DECLARE
    v_message RECORD;
BEGIN
    -- Get message
    SELECT * INTO v_message
    FROM messages
    WHERE id = p_message_id AND user_id = p_user_id AND is_deleted = false;

    IF v_message IS NULL THEN
        RAISE EXCEPTION 'Message not found or you do not have permission to edit it';
    END IF;

    -- Check if message is too old to edit (e.g., 1 hour)
    IF v_message.created_at < CURRENT_TIMESTAMP - INTERVAL '1 hour' THEN
        RAISE EXCEPTION 'Message is too old to edit';
    END IF;

    -- Update message
    UPDATE messages
    SET content = p_new_content,
        edited_at = CURRENT_TIMESTAMP,
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{edit_history}',
            COALESCE(metadata->'edit_history', '[]'::jsonb) ||
            json_build_object(
                'previous_content', v_message.content,
                'edited_at', CURRENT_TIMESTAMP
            )::jsonb
        )
    WHERE id = p_message_id;

    RETURN json_build_object(
        'success', true,
        'message', 'Message edited successfully'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Delete a message
CREATE OR REPLACE FUNCTION delete_message(
    p_message_id UUID,
    p_user_id UUID,
    p_is_moderator BOOLEAN DEFAULT false
) RETURNS JSON AS $$
DECLARE
    v_message RECORD;
BEGIN
    -- Get message
    SELECT m.*, rm.role INTO v_message
    FROM messages m
    LEFT JOIN room_members rm ON rm.room_id = m.room_id AND rm.user_id = p_user_id
    WHERE m.id = p_message_id AND m.is_deleted = false;

    IF v_message IS NULL THEN
        RAISE EXCEPTION 'Message not found';
    END IF;

    -- Check permissions
    IF v_message.user_id != p_user_id AND
       NOT p_is_moderator AND
       v_message.role NOT IN ('owner', 'admin', 'moderator') THEN
        RAISE EXCEPTION 'You do not have permission to delete this message';
    END IF;

    -- Soft delete the message
    UPDATE messages
    SET is_deleted = true,
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{deleted_by}',
            json_build_object(
                'user_id', p_user_id,
                'deleted_at', CURRENT_TIMESTAMP,
                'is_moderator_action', p_is_moderator
            )::jsonb
        )
    WHERE id = p_message_id;

    RETURN json_build_object(
        'success', true,
        'message', 'Message deleted successfully'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- React to a message
CREATE OR REPLACE FUNCTION add_message_reaction(
    p_message_id UUID,
    p_user_id UUID,
    p_emoji VARCHAR
) RETURNS JSON AS $$
BEGIN
    -- Check if user can access this message (member of room)
    IF NOT EXISTS (
        SELECT 1 FROM messages m
        JOIN room_members rm ON rm.room_id = m.room_id
        WHERE m.id = p_message_id
        AND rm.user_id = p_user_id
        AND rm.is_banned = false
        AND m.is_deleted = false
    ) THEN
        RAISE EXCEPTION 'Message not found or access denied';
    END IF;

    -- Add or update reaction
    INSERT INTO message_reactions (message_id, user_id, emoji)
    VALUES (p_message_id, p_user_id, p_emoji)
    ON CONFLICT (message_id, user_id, emoji) DO NOTHING;

    RETURN json_build_object(
        'success', true,
        'message', 'Reaction added'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Remove message reaction
CREATE OR REPLACE FUNCTION remove_message_reaction(
    p_message_id UUID,
    p_user_id UUID,
    p_emoji VARCHAR
) RETURNS JSON AS $$
BEGIN
    DELETE FROM message_reactions
    WHERE message_id = p_message_id
    AND user_id = p_user_id
    AND emoji = p_emoji;

    RETURN json_build_object(
        'success', true,
        'message', 'Reaction removed'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Update user presence
CREATE OR REPLACE FUNCTION update_user_presence(
    p_user_id UUID,
    p_room_id UUID DEFAULT NULL,
    p_status VARCHAR DEFAULT 'online',
    p_session_id VARCHAR DEFAULT NULL
) RETURNS JSON AS $$
BEGIN
    -- Update or insert presence
    INSERT INTO user_presence (user_id, room_id, status, session_id)
    VALUES (p_user_id, p_room_id, p_status, p_session_id)
    ON CONFLICT (user_id, room_id, session_id)
    DO UPDATE SET
        status = EXCLUDED.status,
        last_activity = CURRENT_TIMESTAMP;

    -- Also update user status
    UPDATE users
    SET status = p_status,
        last_seen = CASE WHEN p_status = 'offline' THEN CURRENT_TIMESTAMP ELSE last_seen END
    WHERE id = p_user_id;

    RETURN json_build_object(
        'success', true,
        'message', 'Presence updated'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Set typing indicator
CREATE OR REPLACE FUNCTION set_typing_indicator(
    p_room_id UUID,
    p_user_id UUID,
    p_is_typing BOOLEAN DEFAULT true
) RETURNS JSON AS $$
BEGIN
    IF p_is_typing THEN
        -- Add or update typing indicator
        INSERT INTO typing_indicators (room_id, user_id)
        VALUES (p_room_id, p_user_id)
        ON CONFLICT (room_id, user_id)
        DO UPDATE SET
            started_at = CURRENT_TIMESTAMP,
            expires_at = CURRENT_TIMESTAMP + INTERVAL '10 seconds';
    ELSE
        -- Remove typing indicator
        DELETE FROM typing_indicators
        WHERE room_id = p_room_id AND user_id = p_user_id;
    END IF;

    RETURN json_build_object(
        'success', true,
        'message', 'Typing indicator updated'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Mark messages as read
CREATE OR REPLACE FUNCTION mark_messages_read(
    p_room_id UUID,
    p_user_id UUID,
    p_up_to_message_id UUID DEFAULT NULL
) RETURNS JSON AS $$
DECLARE
    v_timestamp TIMESTAMP WITH TIME ZONE;
BEGIN
    -- Check if user is member of room
    IF NOT EXISTS (
        SELECT 1 FROM room_members
        WHERE room_id = p_room_id AND user_id = p_user_id AND is_banned = false
    ) THEN
        RAISE EXCEPTION 'User is not a member of this room';
    END IF;

    -- Get timestamp of the message or use current time
    IF p_up_to_message_id IS NOT NULL THEN
        SELECT created_at INTO v_timestamp
        FROM messages
        WHERE id = p_up_to_message_id AND room_id = p_room_id;

        IF v_timestamp IS NULL THEN
            RAISE EXCEPTION 'Message not found in this room';
        END IF;
    ELSE
        v_timestamp := CURRENT_TIMESTAMP;
    END IF;

    -- Update last read timestamp
    UPDATE room_members
    SET last_read_at = v_timestamp
    WHERE room_id = p_room_id AND user_id = p_user_id;

    -- Add read receipts for messages
    INSERT INTO message_read_receipts (message_id, user_id)
    SELECT m.id, p_user_id
    FROM messages m
    WHERE m.room_id = p_room_id
    AND m.created_at <= v_timestamp
    AND m.user_id != p_user_id
    AND NOT EXISTS (
        SELECT 1 FROM message_read_receipts mrr
        WHERE mrr.message_id = m.id AND mrr.user_id = p_user_id
    )
    ON CONFLICT (message_id, user_id) DO NOTHING;

    RETURN json_build_object(
        'success', true,
        'message', 'Messages marked as read'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Create direct conversation
CREATE OR REPLACE FUNCTION create_direct_conversation(
    p_user1_id UUID,
    p_user2_id UUID
) RETURNS JSON AS $$
DECLARE
    v_room_id UUID;
    v_conversation_id UUID;
    v_ordered_user1 UUID;
    v_ordered_user2 UUID;
BEGIN
    -- Ensure consistent ordering
    IF p_user1_id < p_user2_id THEN
        v_ordered_user1 := p_user1_id;
        v_ordered_user2 := p_user2_id;
    ELSE
        v_ordered_user1 := p_user2_id;
        v_ordered_user2 := p_user1_id;
    END IF;

    -- Check if conversation already exists
    SELECT room_id INTO v_room_id
    FROM direct_conversations
    WHERE user1_id = v_ordered_user1 AND user2_id = v_ordered_user2;

    IF v_room_id IS NOT NULL THEN
        RETURN json_build_object(
            'success', true,
            'room_id', v_room_id,
            'message', 'Direct conversation already exists'
        );
    END IF;

    -- Create room for direct conversation
    INSERT INTO rooms (name, slug, type, owner_id, max_members)
    VALUES (
        'Direct Message',
        'dm-' || v_ordered_user1 || '-' || v_ordered_user2,
        'direct',
        v_ordered_user1,
        2
    ) RETURNING id INTO v_room_id;

    -- Create conversation record
    INSERT INTO direct_conversations (room_id, user1_id, user2_id)
    VALUES (v_room_id, v_ordered_user1, v_ordered_user2)
    RETURNING id INTO v_conversation_id;

    -- Add both users as members
    INSERT INTO room_members (room_id, user_id, role) VALUES
    (v_room_id, v_ordered_user1, 'member'),
    (v_room_id, v_ordered_user2, 'member');

    RETURN json_build_object(
        'success', true,
        'room_id', v_room_id,
        'conversation_id', v_conversation_id,
        'message', 'Direct conversation created'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;
