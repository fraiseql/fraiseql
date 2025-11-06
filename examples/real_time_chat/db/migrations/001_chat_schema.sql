-- Real-time Chat Database Schema
-- Demonstrates FraiseQL's capabilities with WebSocket subscriptions and PostgreSQL LISTEN/NOTIFY

-- Enable necessary extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm"; -- For message search

-- Users table
CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    username VARCHAR(50) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    display_name VARCHAR(100),
    avatar_url TEXT,
    status VARCHAR(20) DEFAULT 'offline' CHECK (status IN ('online', 'away', 'busy', 'offline')),
    last_seen TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT true,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Chat rooms/channels
CREATE TABLE tb_room (
    pk_room INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    type VARCHAR(20) NOT NULL DEFAULT 'public' CHECK (type IN ('public', 'private', 'direct')),
    fk_owner INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    max_members INTEGER DEFAULT 1000,
    is_active BOOLEAN DEFAULT true,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Room membership
CREATE TABLE tb_room_member (
    pk_room_member INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_room INT NOT NULL REFERENCES tb_room(pk_room) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    role VARCHAR(20) DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'moderator', 'member')),
    joined_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    last_read_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_muted BOOLEAN DEFAULT false,
    is_banned BOOLEAN DEFAULT false,
    ban_expires_at TIMESTAMP WITH TIME ZONE,
    UNIQUE(fk_room, fk_user)
);

-- Messages
CREATE TABLE tb_message (
    pk_message INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_room INT NOT NULL REFERENCES tb_room(pk_room) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    content TEXT NOT NULL,
    message_type VARCHAR(20) DEFAULT 'text' CHECK (message_type IN ('text', 'image', 'file', 'system')),
    fk_parent_message INT REFERENCES tb_message(pk_message) ON DELETE SET NULL, -- For threading/replies
    edited_at TIMESTAMP WITH TIME ZONE,
    is_deleted BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}', -- For mentions, formatting, etc.
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Message attachments
CREATE TABLE tb_message_attachment (
    pk_message_attachment INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_message INT NOT NULL REFERENCES tb_message(pk_message) ON DELETE CASCADE,
    filename VARCHAR(255) NOT NULL,
    original_filename VARCHAR(255) NOT NULL,
    file_size BIGINT NOT NULL,
    mime_type VARCHAR(100) NOT NULL,
    url TEXT NOT NULL,
    thumbnail_url TEXT,
    width INTEGER, -- For images
    height INTEGER, -- For images
    duration INTEGER, -- For audio/video in seconds
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Message reactions (emojis)
CREATE TABLE tb_message_reaction (
    pk_message_reaction INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_message INT NOT NULL REFERENCES tb_message(pk_message) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    emoji VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(fk_message, fk_user, emoji)
);

-- Direct message conversations (for 1-on-1 chats)
CREATE TABLE tb_direct_conversation (
    pk_direct_conversation INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_room INT NOT NULL REFERENCES tb_room(pk_room) ON DELETE CASCADE,
    fk_user1 INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    fk_user2 INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(fk_user1, fk_user2),
    CHECK (fk_user1 < fk_user2) -- Ensure consistent ordering
);

-- User presence tracking
CREATE TABLE tb_user_presence (
    pk_user_presence INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    fk_room INT REFERENCES tb_room(pk_room) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL CHECK (status IN ('online', 'away', 'typing')),
    last_activity TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    session_id VARCHAR(255),
    metadata JSONB DEFAULT '{}',
    UNIQUE(fk_user, fk_room, session_id)
);

-- Typing indicators
CREATE TABLE tb_typing_indicator (
    pk_typing_indicator INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_room INT NOT NULL REFERENCES tb_room(pk_room) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    started_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE DEFAULT (CURRENT_TIMESTAMP + INTERVAL '10 seconds'),
    UNIQUE(fk_room, fk_user)
);

-- Message read receipts
CREATE TABLE tb_message_read_receipt (
    pk_message_read_receipt INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_message INT NOT NULL REFERENCES tb_message(pk_message) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    read_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(fk_message, fk_user)
);

-- Push notification subscriptions
CREATE TABLE tb_push_subscription (
    pk_push_subscription INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    endpoint TEXT NOT NULL,
    keys JSONB NOT NULL,
    user_agent TEXT,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Moderation logs
CREATE TABLE tb_moderation_log (
    pk_moderation_log INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_room INT NOT NULL REFERENCES tb_room(pk_room) ON DELETE CASCADE,
    fk_moderator INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    fk_target_user INT REFERENCES tb_user(pk_user) ON DELETE SET NULL,
    fk_target_message INT REFERENCES tb_message(pk_message) ON DELETE SET NULL,
    action VARCHAR(50) NOT NULL, -- ban, unban, kick, delete_message, etc.
    reason TEXT,
    duration INTERVAL, -- For temporary actions
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for performance
CREATE INDEX idx_tb_message_fk_room_created ON tb_message(fk_room, created_at DESC);
CREATE INDEX idx_tb_message_fk_user ON tb_message(fk_user);
CREATE INDEX idx_tb_message_parent ON tb_message(fk_parent_message) WHERE fk_parent_message IS NOT NULL;
CREATE INDEX idx_tb_message_content_search ON tb_message USING gin(to_tsvector('english', content)) WHERE is_deleted = false;

CREATE INDEX idx_tb_room_member_fk_room ON tb_room_member(fk_room) WHERE is_banned = false;
CREATE INDEX idx_tb_room_member_fk_user ON tb_room_member(fk_user);
CREATE INDEX idx_tb_room_member_last_read ON tb_room_member(fk_room, last_read_at);

CREATE INDEX idx_tb_message_reaction_fk_message ON tb_message_reaction(fk_message);
CREATE INDEX idx_tb_message_reaction_fk_user ON tb_message_reaction(fk_user);

CREATE INDEX idx_tb_user_presence_fk_user ON tb_user_presence(fk_user);
CREATE INDEX idx_tb_user_presence_fk_room ON tb_user_presence(fk_room) WHERE fk_room IS NOT NULL;
CREATE INDEX idx_tb_user_presence_active ON tb_user_presence(fk_user, last_activity) WHERE status = 'online';

CREATE INDEX idx_tb_typing_indicator_fk_room ON tb_typing_indicator(fk_room) WHERE expires_at > CURRENT_TIMESTAMP;
CREATE INDEX idx_tb_typing_indicator_expires ON tb_typing_indicator(expires_at);

CREATE INDEX idx_tb_message_read_receipt_fk_message ON tb_message_read_receipt(fk_message);
CREATE INDEX idx_tb_message_read_receipt_fk_user ON tb_message_read_receipt(fk_user);

-- Update timestamp triggers
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_tb_user_updated_at BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_room_updated_at BEFORE UPDATE ON tb_room
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_push_subscription_updated_at BEFORE UPDATE ON tb_push_subscription
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Notification triggers for real-time subscriptions
CREATE OR REPLACE FUNCTION notify_message_event()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify(
        'message_event',
        json_build_object(
            'event', TG_OP,
            'room_id', COALESCE(NEW.fk_room, OLD.fk_room),
            'message_id', COALESCE(NEW.id, OLD.id),
            'user_id', COALESCE(NEW.fk_user, OLD.fk_user),
            'timestamp', CURRENT_TIMESTAMP
        )::text
    );
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER message_event_trigger
    AFTER INSERT OR UPDATE OR DELETE ON tb_message
    FOR EACH ROW EXECUTE FUNCTION notify_message_event();

-- Typing indicator notification
CREATE OR REPLACE FUNCTION notify_typing_event()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify(
        'typing_event',
        json_build_object(
            'event', TG_OP,
            'room_id', COALESCE(NEW.fk_room, OLD.fk_room),
            'user_id', COALESCE(NEW.fk_user, OLD.fk_user),
            'timestamp', CURRENT_TIMESTAMP
        )::text
    );
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER typing_event_trigger
    AFTER INSERT OR UPDATE OR DELETE ON tb_typing_indicator
    FOR EACH ROW EXECUTE FUNCTION notify_typing_event();

-- User presence notification
CREATE OR REPLACE FUNCTION notify_presence_event()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify(
        'presence_event',
        json_build_object(
            'event', TG_OP,
            'user_id', COALESCE(NEW.fk_user, OLD.fk_user),
            'room_id', COALESCE(NEW.fk_room, OLD.fk_room),
            'status', COALESCE(NEW.status, OLD.status),
            'timestamp', CURRENT_TIMESTAMP
        )::text
    );
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER presence_event_trigger
    AFTER INSERT OR UPDATE OR DELETE ON tb_user_presence
    FOR EACH ROW EXECUTE FUNCTION notify_presence_event();

-- Cleanup functions
CREATE OR REPLACE FUNCTION cleanup_expired_typing_indicators()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM typing_indicators WHERE expires_at < CURRENT_TIMESTAMP;
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Auto-cleanup of old presence records
CREATE OR REPLACE FUNCTION cleanup_old_presence_records()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM user_presence
    WHERE last_activity < CURRENT_TIMESTAMP - INTERVAL '1 hour'
    AND status != 'online';
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;
