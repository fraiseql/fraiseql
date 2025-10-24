"""Real-time Chat API Models

Demonstrates FraiseQL's real-time capabilities with WebSocket subscriptions
"""

from datetime import datetime
from typing import Any
from uuid import UUID

from pydantic import BaseModel, Field

from fraiseql import QueryType, register_type


# Base Types
class User(BaseModel):
    id: UUID
    username: str
    email: str
    display_name: str | None = None
    avatar_url: str | None = None
    status: str = "offline"  # online, away, busy, offline
    last_seen: datetime
    is_active: bool = True
    metadata: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class Room(BaseModel):
    id: UUID
    name: str
    slug: str
    description: str | None = None
    type: str  # public, private, direct
    owner_id: UUID
    max_members: int = 1000
    is_active: bool = True
    settings: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class RoomMember(BaseModel):
    id: UUID
    room_id: UUID
    user_id: UUID
    role: str = "member"  # owner, admin, moderator, member
    joined_at: datetime
    last_read_at: datetime
    is_muted: bool = False
    is_banned: bool = False
    ban_expires_at: datetime | None = None


class Message(BaseModel):
    id: UUID
    room_id: UUID
    user_id: UUID
    content: str
    message_type: str = "text"  # text, image, file, system
    parent_message_id: UUID | None = None
    edited_at: datetime | None = None
    is_deleted: bool = False
    metadata: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime


class MessageAttachment(BaseModel):
    id: UUID
    message_id: UUID
    filename: str
    original_filename: str
    file_size: int
    mime_type: str
    url: str
    thumbnail_url: str | None = None
    width: int | None = None
    height: int | None = None
    duration: int | None = None  # For audio/video
    created_at: datetime


class MessageReaction(BaseModel):
    id: UUID
    message_id: UUID
    user_id: UUID
    emoji: str
    created_at: datetime


class UserPresence(BaseModel):
    id: UUID
    user_id: UUID
    room_id: UUID | None = None
    status: str  # online, away, typing
    last_activity: datetime
    session_id: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class TypingIndicator(BaseModel):
    id: UUID
    room_id: UUID
    user_id: UUID
    started_at: datetime
    expires_at: datetime


class DirectConversation(BaseModel):
    id: UUID
    room_id: UUID
    user1_id: UUID
    user2_id: UUID
    created_at: datetime


# Enhanced Views
class RoomList(Room):
    owner: dict[str, Any]
    member_count: int = 0
    online_count: int = 0
    latest_message: dict[str, Any | None] = None


class RoomDetail(Room):
    owner: dict[str, Any]
    members: list[dict[str, Any]] = Field(default_factory=list)
    member_count: int = 0
    message_count: int = 0
    online_count: int = 0


class MessageThread(Message):
    author: dict[str, Any]
    attachments: list[dict[str, Any]] = Field(default_factory=list)
    reactions: list[dict[str, Any]] = Field(default_factory=list)
    reply_count: int = 0
    read_count: int = 0


class UserConversation(BaseModel):
    user_id: UUID
    room_id: UUID
    name: str
    slug: str
    type: str
    description: str | None = None
    role: str
    joined_at: datetime
    last_read_at: datetime
    is_muted: bool
    unread_count: int = 0
    latest_message: dict[str, Any | None] = None
    direct_user: dict[str, Any | None] = None  # For direct conversations


class OnlineUser(User):
    active_rooms: list[dict[str, Any]] = Field(default_factory=list)


class ActiveTyping(BaseModel):
    room_id: UUID
    typing_users: list[dict[str, Any]] = Field(default_factory=list)


class MessageSearch(Message):
    room: dict[str, Any]
    author: dict[str, Any]
    search_rank: float | None = None


class RoomAnalytics(BaseModel):
    room_id: UUID
    name: str
    type: str
    created_date: datetime
    total_messages: int = 0
    messages_last_7_days: int = 0
    messages_last_30_days: int = 0
    total_members: int = 0
    active_users_7_days: int = 0
    active_users_30_days: int = 0
    avg_daily_messages: float | None = None
    peak_daily_messages: int | None = None


# Mutation Result Types
class MutationResult(BaseModel):
    success: bool
    message: str | None = None
    error: str | None = None


class RoomMutationResult(MutationResult):
    room_id: UUID | None = None


class MessageMutationResult(MutationResult):
    message_id: UUID | None = None


class ConversationMutationResult(MutationResult):
    room_id: UUID | None = None
    conversation_id: UUID | None = None


# Subscription Event Types
class MessageEvent(BaseModel):
    event: str  # INSERT, UPDATE, DELETE
    room_id: UUID
    message_id: UUID
    user_id: UUID
    timestamp: datetime
    message: MessageThread | None = None


class TypingEvent(BaseModel):
    event: str  # INSERT, UPDATE, DELETE
    room_id: UUID
    user_id: UUID
    timestamp: datetime
    user: dict[str, Any | None] = None


class PresenceEvent(BaseModel):
    event: str  # INSERT, UPDATE, DELETE
    user_id: UUID
    room_id: UUID | None = None
    status: str
    timestamp: datetime
    user: dict[str, Any | None] = None


# WebSocket Message Types
class WebSocketMessage(BaseModel):
    type: str
    payload: dict[str, Any]
    timestamp: datetime = Field(default_factory=datetime.now)


class RoomSubscription(BaseModel):
    room_id: UUID
    user_id: UUID
    session_id: str


class PushSubscription(BaseModel):
    id: UUID
    user_id: UUID
    endpoint: str
    keys: dict[str, str]
    user_agent: str | None = None
    is_active: bool = True
    created_at: datetime
    updated_at: datetime


class ModerationLog(BaseModel):
    id: UUID
    room_id: UUID
    moderator_id: UUID
    target_user_id: UUID | None = None
    target_message_id: UUID | None = None
    action: str  # ban, unban, kick, delete_message, etc.
    reason: str | None = None
    duration: str | None = None  # For temporary actions
    metadata: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime


# Register all types with FraiseQL
@register_type
class ChatQuery(QueryType):
    # User queries
    users: list[User]
    online_users: list[OnlineUser]
    user_presence: list[UserPresence]

    # Room queries
    rooms: list[Room]
    room_list: list[RoomList]
    room_detail: list[RoomDetail]
    user_conversations: list[UserConversation]

    # Message queries
    messages: list[Message]
    message_thread: list[MessageThread]
    message_search: list[MessageSearch]

    # Real-time queries
    active_typing: list[ActiveTyping]

    # Analytics
    room_analytics: list[RoomAnalytics]

    # Direct messages
    direct_conversations: list[DirectConversation]

    # Moderation
    moderation_logs: list[ModerationLog]
