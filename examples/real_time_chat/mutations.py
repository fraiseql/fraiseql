"""Real-time Chat API Mutations

Demonstrates FraiseQL's mutation system with real-time features
"""

from uuid import UUID

from fraiseql import mutation

from .models import (
    ConversationMutationResult,
    MessageMutationResult,
    MutationResult,
    RoomMutationResult,
)


# Room Management Mutations
@mutation(
    name="createRoom",
    function="create_room",
    description="Create a new chat room",
)
async def create_room(
    name: str,
    slug: str,
    owner_id: UUID,
    description: str | None = None,
    type: str = "public",
    max_members: int = 1000,
    settings: dict[str, Any] | None = None,
) -> RoomMutationResult:
    """Create a new chat room"""


@mutation(name="joinRoom", function="join_room", description="Join a chat room")
async def join_room(
    room_id: UUID,
    user_id: UUID,
    role: str = "member",
) -> MutationResult:
    """Join an existing chat room"""


# Message Mutations
@mutation(
    name="sendMessage",
    function="send_message",
    description="Send a message to a room",
)
async def send_message(
    room_id: UUID,
    user_id: UUID,
    content: str,
    message_type: str = "text",
    parent_message_id: UUID | None = None,
    metadata: dict[str, Any] | None = None,
) -> MessageMutationResult:
    """Send a message to a chat room"""


@mutation(
    name="editMessage",
    function="edit_message",
    description="Edit an existing message",
)
async def edit_message(
    message_id: UUID,
    user_id: UUID,
    new_content: str,
) -> MutationResult:
    """Edit a message (within time limit)"""


@mutation(
    name="deleteMessage",
    function="delete_message",
    description="Delete a message",
)
async def delete_message(
    message_id: UUID,
    user_id: UUID,
    is_moderator: bool = False,
) -> MutationResult:
    """Delete a message (soft delete)"""


# Reaction Mutations
@mutation(
    name="addMessageReaction",
    function="add_message_reaction",
    description="Add an emoji reaction to a message",
)
async def add_message_reaction(
    message_id: UUID,
    user_id: UUID,
    emoji: str,
) -> MutationResult:
    """Add emoji reaction to a message"""


@mutation(
    name="removeMessageReaction",
    function="remove_message_reaction",
    description="Remove an emoji reaction from a message",
)
async def remove_message_reaction(
    message_id: UUID,
    user_id: UUID,
    emoji: str,
) -> MutationResult:
    """Remove emoji reaction from a message"""


# Presence Mutations
@mutation(
    name="updateUserPresence",
    function="update_user_presence",
    description="Update user presence status",
)
async def update_user_presence(
    user_id: UUID,
    status: str = "online",
    room_id: UUID | None = None,
    session_id: str | None = None,
) -> MutationResult:
    """Update user online presence"""


@mutation(
    name="setTypingIndicator",
    function="set_typing_indicator",
    description="Set or clear typing indicator",
)
async def set_typing_indicator(
    room_id: UUID,
    user_id: UUID,
    is_typing: bool = True,
) -> MutationResult:
    """Set or clear typing indicator"""


# Read Status Mutations
@mutation(
    name="markMessagesRead",
    function="mark_messages_read",
    description="Mark messages as read up to a certain point",
)
async def mark_messages_read(
    room_id: UUID,
    user_id: UUID,
    up_to_message_id: UUID | None = None,
) -> MutationResult:
    """Mark messages as read in a room"""


# Direct Message Mutations
@mutation(
    name="createDirectConversation",
    function="create_direct_conversation",
    description="Create or get a direct message conversation",
)
async def create_direct_conversation(
    user1_id: UUID,
    user2_id: UUID,
) -> ConversationMutationResult:
    """Create or retrieve a direct message conversation"""
