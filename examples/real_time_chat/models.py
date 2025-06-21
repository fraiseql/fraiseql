"""Real-time Chat API Models

Demonstrates FraiseQL's real-time capabilities with WebSocket subscriptions
"""

from datetime import datetime
from typing import Any, Dict, List, Optional
from uuid import UUID

from pydantic import BaseModel, Field

from fraiseql import QueryType, register_type


# Base Types
class User(BaseModel):
    id: UUID
    username: str
    email: str
    display_name: Optional[str] = None
    avatar_url: Optional[str] = None
    status: str = "offline"  # online, away, busy, offline
    last_seen: datetime
    is_active: bool = True
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class Room(BaseModel):
    id: UUID
    name: str
    slug: str
    description: Optional[str] = None
    type: str  # public, private, direct
    owner_id: UUID
    max_members: int = 1000
    is_active: bool = True
    settings: Dict[str, Any] = Field(default_factory=dict)
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
    ban_expires_at: Optional[datetime] = None


class Message(BaseModel):
    id: UUID
    room_id: UUID
    user_id: UUID
    content: str
    message_type: str = "text"  # text, image, file, system
    parent_message_id: Optional[UUID] = None
    edited_at: Optional[datetime] = None
    is_deleted: bool = False
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime


class MessageAttachment(BaseModel):
    id: UUID
    message_id: UUID
    filename: str
    original_filename: str
    file_size: int
    mime_type: str
    url: str
    thumbnail_url: Optional[str] = None
    width: Optional[int] = None
    height: Optional[int] = None
    duration: Optional[int] = None  # For audio/video
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
    room_id: Optional[UUID] = None
    status: str  # online, away, typing
    last_activity: datetime
    session_id: Optional[str] = None
    metadata: Dict[str, Any] = Field(default_factory=dict)


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
    owner: Dict[str, Any]
    member_count: int = 0
    online_count: int = 0
    latest_message: Optional[Dict[str, Any]] = None


class RoomDetail(Room):
    owner: Dict[str, Any]
    members: List[Dict[str, Any]] = Field(default_factory=list)
    member_count: int = 0
    message_count: int = 0
    online_count: int = 0


class MessageThread(Message):
    author: Dict[str, Any]
    attachments: List[Dict[str, Any]] = Field(default_factory=list)
    reactions: List[Dict[str, Any]] = Field(default_factory=list)
    reply_count: int = 0
    read_count: int = 0


class UserConversation(BaseModel):
    user_id: UUID
    room_id: UUID
    name: str
    slug: str
    type: str
    description: Optional[str] = None
    role: str
    joined_at: datetime
    last_read_at: datetime
    is_muted: bool
    unread_count: int = 0
    latest_message: Optional[Dict[str, Any]] = None
    direct_user: Optional[Dict[str, Any]] = None  # For direct conversations


class OnlineUser(User):
    active_rooms: List[Dict[str, Any]] = Field(default_factory=list)


class ActiveTyping(BaseModel):
    room_id: UUID
    typing_users: List[Dict[str, Any]] = Field(default_factory=list)


class MessageSearch(Message):
    room: Dict[str, Any]
    author: Dict[str, Any]
    search_rank: Optional[float] = None


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
    avg_daily_messages: Optional[float] = None
    peak_daily_messages: Optional[int] = None


# Mutation Result Types
class MutationResult(BaseModel):
    success: bool
    message: Optional[str] = None
    error: Optional[str] = None


class RoomMutationResult(MutationResult):
    room_id: Optional[UUID] = None


class MessageMutationResult(MutationResult):
    message_id: Optional[UUID] = None


class ConversationMutationResult(MutationResult):
    room_id: Optional[UUID] = None
    conversation_id: Optional[UUID] = None


# Subscription Event Types
class MessageEvent(BaseModel):
    event: str  # INSERT, UPDATE, DELETE
    room_id: UUID
    message_id: UUID
    user_id: UUID
    timestamp: datetime
    message: Optional[MessageThread] = None


class TypingEvent(BaseModel):
    event: str  # INSERT, UPDATE, DELETE
    room_id: UUID
    user_id: UUID
    timestamp: datetime
    user: Optional[Dict[str, Any]] = None


class PresenceEvent(BaseModel):
    event: str  # INSERT, UPDATE, DELETE
    user_id: UUID
    room_id: Optional[UUID] = None
    status: str
    timestamp: datetime
    user: Optional[Dict[str, Any]] = None


# WebSocket Message Types
class WebSocketMessage(BaseModel):
    type: str
    payload: Dict[str, Any]
    timestamp: datetime = Field(default_factory=datetime.now)


class RoomSubscription(BaseModel):
    room_id: UUID
    user_id: UUID
    session_id: str


class PushSubscription(BaseModel):
    id: UUID
    user_id: UUID
    endpoint: str
    keys: Dict[str, str]
    user_agent: Optional[str] = None
    is_active: bool = True
    created_at: datetime
    updated_at: datetime


class ModerationLog(BaseModel):
    id: UUID
    room_id: UUID
    moderator_id: UUID
    target_user_id: Optional[UUID] = None
    target_message_id: Optional[UUID] = None
    action: str  # ban, unban, kick, delete_message, etc.
    reason: Optional[str] = None
    duration: Optional[str] = None  # For temporary actions
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime


# Register all types with FraiseQL
@register_type
class ChatQuery(QueryType):
    # User queries
    users: List[User]
    online_users: List[OnlineUser]
    user_presence: List[UserPresence]

    # Room queries
    rooms: List[Room]
    room_list: List[RoomList]
    room_detail: List[RoomDetail]
    user_conversations: List[UserConversation]

    # Message queries
    messages: List[Message]
    message_thread: List[MessageThread]
    message_search: List[MessageSearch]

    # Real-time queries
    active_typing: List[ActiveTyping]

    # Analytics
    room_analytics: List[RoomAnalytics]

    # Direct messages
    direct_conversations: List[DirectConversation]

    # Moderation
    moderation_logs: List[ModerationLog]
