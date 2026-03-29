-- Chat Functions for Real-time Chat API
-- CQRS pattern: Functions for mutations
-- All functions return mutation_response type

-- Create a new room
CREATE OR REPLACE FUNCTION create_room(
    p_name VARCHAR,
    p_slug VARCHAR,
    p_description TEXT,
    p_type VARCHAR,
    p_owner_id UUID,
    p_max_members INTEGER DEFAULT 1000,
    p_settings JSONB DEFAULT '{}'
) RETURNS mutation_response AS $$
DECLARE
    v_room_id UUID;
BEGIN
    -- Check if slug is available
    IF EXISTS (SELECT 1 FROM rooms WHERE slug = p_slug) THEN
        RETURN ROW('failed:validation', 'Room slug already exists', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Validate room type
    IF p_type NOT IN ('public', 'private', 'direct') THEN
        RETURN ROW('failed:validation', 'Invalid room type', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Create room
    INSERT INTO rooms (name, slug, description, type, owner_id, max_members, settings)
    VALUES (p_name, p_slug, p_description, p_type, p_owner_id, p_max_members, p_settings)
    RETURNING id INTO v_room_id;

    -- Add owner as admin member
    INSERT INTO room_members (room_id, user_id, role)
    VALUES (v_room_id, p_owner_id, 'owner');

    RETURN ROW(
        'new',
        'Room created successfully',
        v_room_id::text,
        'Room',
        jsonb_build_object('id', v_room_id, 'name', p_name, 'slug', p_slug, 'type', p_type),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Join a room
CREATE OR REPLACE FUNCTION join_room(
    p_room_id UUID,
    p_user_id UUID,
    p_role VARCHAR DEFAULT 'member'
) RETURNS mutation_response AS $$
DECLARE
    v_room RECORD;
    v_member_count INTEGER;
BEGIN
    -- Get room info
    SELECT * INTO v_room FROM rooms WHERE id = p_room_id AND is_active = true;

    IF v_room IS NULL THEN
        RETURN ROW('failed:validation', 'Room not found or inactive', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check if already a member
    IF EXISTS (
        SELECT 1 FROM room_members
        WHERE room_id = p_room_id AND user_id = p_user_id
    ) THEN
        RETURN ROW('failed:validation', 'User is already a member', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check room capacity
    SELECT COUNT(*) INTO v_member_count
    FROM room_members
    WHERE room_id = p_room_id AND is_banned = false;

    IF v_member_count >= v_room.max_members THEN
        RETURN ROW('failed:validation', 'Room is at maximum capacity', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
        jsonb_build_object('action', 'user_joined')
    );

    RETURN ROW(
        'success',
        'Successfully joined room',
        p_room_id::text,
        'Room',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
) RETURNS mutation_response AS $$
DECLARE
    v_message_id UUID;
    v_room_member RECORD;
BEGIN
    -- Check if user is a member of the room
    SELECT * INTO v_room_member
    FROM room_members
    WHERE room_id = p_room_id AND user_id = p_user_id AND is_banned = false;

    IF v_room_member IS NULL THEN
        RETURN ROW('failed:validation', 'User is not a member of this room or is banned', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Validate message type
    IF p_message_type NOT IN ('text', 'image', 'file', 'system') THEN
        RETURN ROW('failed:validation', 'Invalid message type', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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

    RETURN ROW(
        'new',
        'Message sent successfully',
        v_message_id::text,
        'Message',
        jsonb_build_object('id', v_message_id, 'room_id', p_room_id, 'content', p_content),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Edit a message
CREATE OR REPLACE FUNCTION edit_message(
    p_message_id UUID,
    p_user_id UUID,
    p_new_content TEXT
) RETURNS mutation_response AS $$
DECLARE
    v_message RECORD;
BEGIN
    -- Get message
    SELECT * INTO v_message
    FROM messages
    WHERE id = p_message_id AND user_id = p_user_id AND is_deleted = false;

    IF v_message IS NULL THEN
        RETURN ROW('failed:validation', 'Message not found or you do not have permission to edit it', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check if message is too old to edit (e.g., 1 hour)
    IF v_message.created_at < CURRENT_TIMESTAMP - INTERVAL '1 hour' THEN
        RETURN ROW('failed:validation', 'Message is too old to edit', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Update message
    UPDATE messages
    SET content = p_new_content,
        edited_at = CURRENT_TIMESTAMP,
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{edit_history}',
            COALESCE(metadata->'edit_history', '[]'::jsonb) ||
            jsonb_build_object(
                'previous_content', v_message.content,
                'edited_at', CURRENT_TIMESTAMP
            )
        )
    WHERE id = p_message_id;

    RETURN ROW(
        'success',
        'Message edited successfully',
        p_message_id::text,
        'Message',
        jsonb_build_object('id', p_message_id, 'content', p_new_content),
        ARRAY['content']::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Delete a message
CREATE OR REPLACE FUNCTION delete_message(
    p_message_id UUID,
    p_user_id UUID,
    p_is_moderator BOOLEAN DEFAULT false
) RETURNS mutation_response AS $$
DECLARE
    v_message RECORD;
BEGIN
    -- Get message
    SELECT m.*, rm.role INTO v_message
    FROM messages m
    LEFT JOIN room_members rm ON rm.room_id = m.room_id AND rm.user_id = p_user_id
    WHERE m.id = p_message_id AND m.is_deleted = false;

    IF v_message IS NULL THEN
        RETURN ROW('failed:validation', 'Message not found', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check permissions
    IF v_message.user_id != p_user_id AND
       NOT p_is_moderator AND
       v_message.role NOT IN ('owner', 'admin', 'moderator') THEN
        RETURN ROW('failed:validation', 'You do not have permission to delete this message', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Soft delete the message
    UPDATE messages
    SET is_deleted = true,
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{deleted_by}',
            jsonb_build_object(
                'user_id', p_user_id,
                'deleted_at', CURRENT_TIMESTAMP,
                'is_moderator_action', p_is_moderator
            )
        )
    WHERE id = p_message_id;

    RETURN ROW(
        'success',
        'Message deleted successfully',
        p_message_id::text,
        'Message',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- React to a message
CREATE OR REPLACE FUNCTION add_message_reaction(
    p_message_id UUID,
    p_user_id UUID,
    p_emoji VARCHAR
) RETURNS mutation_response AS $$
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
        RETURN ROW('failed:validation', 'Message not found or access denied', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Add or update reaction
    INSERT INTO message_reactions (message_id, user_id, emoji)
    VALUES (p_message_id, p_user_id, p_emoji)
    ON CONFLICT (message_id, user_id, emoji) DO NOTHING;

    RETURN ROW(
        'new',
        'Reaction added',
        NULL::text,
        'Reaction',
        jsonb_build_object('message_id', p_message_id, 'emoji', p_emoji),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Remove message reaction
CREATE OR REPLACE FUNCTION remove_message_reaction(
    p_message_id UUID,
    p_user_id UUID,
    p_emoji VARCHAR
) RETURNS mutation_response AS $$
BEGIN
    DELETE FROM message_reactions
    WHERE message_id = p_message_id
    AND user_id = p_user_id
    AND emoji = p_emoji;

    RETURN ROW(
        'success',
        'Reaction removed',
        NULL::text,
        'Reaction',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Update user presence
CREATE OR REPLACE FUNCTION update_user_presence(
    p_user_id UUID,
    p_room_id UUID DEFAULT NULL,
    p_status VARCHAR DEFAULT 'online',
    p_session_id VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
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

    RETURN ROW(
        'success',
        'Presence updated',
        p_user_id::text,
        'Presence',
        jsonb_build_object('user_id', p_user_id, 'status', p_status),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Set typing indicator
CREATE OR REPLACE FUNCTION set_typing_indicator(
    p_room_id UUID,
    p_user_id UUID,
    p_is_typing BOOLEAN DEFAULT true
) RETURNS mutation_response AS $$
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

    RETURN ROW(
        'success',
        'Typing indicator updated',
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Mark messages as read
CREATE OR REPLACE FUNCTION mark_messages_read(
    p_room_id UUID,
    p_user_id UUID,
    p_up_to_message_id UUID DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    v_timestamp TIMESTAMP WITH TIME ZONE;
BEGIN
    -- Check if user is member of room
    IF NOT EXISTS (
        SELECT 1 FROM room_members
        WHERE room_id = p_room_id AND user_id = p_user_id AND is_banned = false
    ) THEN
        RETURN ROW('failed:validation', 'User is not a member of this room', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Get timestamp of the message or use current time
    IF p_up_to_message_id IS NOT NULL THEN
        SELECT created_at INTO v_timestamp
        FROM messages
        WHERE id = p_up_to_message_id AND room_id = p_room_id;

        IF v_timestamp IS NULL THEN
            RETURN ROW('failed:validation', 'Message not found in this room', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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

    RETURN ROW(
        'success',
        'Messages marked as read',
        NULL::text,
        NULL::text,
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Create direct conversation
CREATE OR REPLACE FUNCTION create_direct_conversation(
    p_user1_id UUID,
    p_user2_id UUID
) RETURNS mutation_response AS $$
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
        RETURN ROW(
            'success',
            'Direct conversation already exists',
            v_room_id::text,
            'Conversation',
            jsonb_build_object('room_id', v_room_id),
            NULL::text[],
            NULL::jsonb,
            NULL::jsonb
        )::mutation_response;
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

    RETURN ROW(
        'new',
        'Direct conversation created',
        v_conversation_id::text,
        'Conversation',
        jsonb_build_object('room_id', v_room_id, 'conversation_id', v_conversation_id),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;
