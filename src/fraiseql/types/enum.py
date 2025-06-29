"""Enum support for FraiseQL GraphQL schemas.

This module provides the @fraise_enum decorator for defining GraphQL enum types
from Python Enum classes. It handles registration, type conversion, and
serialization/deserialization of enum values.
"""

from collections.abc import Callable
from enum import Enum
from typing import TypeVar, overload

from graphql import GraphQLEnumType, GraphQLEnumValue

T = TypeVar("T", bound=type[Enum])


@overload
def fraise_enum(_cls: None = None) -> Callable[[T], T]: ...
@overload
def fraise_enum(_cls: T) -> T: ...


def fraise_enum(_cls: T | None = None) -> T | Callable[[T], T]:
    """Decorator for GraphQL enum types.

    Converts a Python Enum class into a GraphQL enum type that can be used
    in queries, mutations, and type definitions. Enum values are serialized
    using their names, providing type-safe string constants in GraphQL.

    Args:
        _cls: The Enum class to decorate (when used without parentheses)

    Returns:
        The decorated Enum class registered as a GraphQL enum type

    Examples:
        Basic enum for user roles::\
        
            @fraise_enum
            class UserRole(Enum):
                ADMIN = "admin"
                USER = "user"
                GUEST = "guest"

            @fraise_type
            class User:
                name: str
                role: UserRole
                
            # GraphQL schema will include:
            # enum UserRole {
            #   ADMIN
            #   USER
            #   GUEST
            # }

        Enum with descriptive values::\
        
            @fraise_enum
            class OrderStatus(Enum):
                PENDING = "pending"
                PROCESSING = "processing"
                SHIPPED = "shipped"
                DELIVERED = "delivered"
                CANCELLED = "cancelled"

            @fraise_type
            class Order:
                id: UUID
                status: OrderStatus
                created_at: datetime
                
            # Can be used in mutations:
            @mutation
            async def update_order_status(
                info, 
                order_id: UUID, 
                status: OrderStatus
            ) -> Order:
                db = info.context["db"]
                return await db.update_one(
                    "order_view", 
                    {"id": order_id}, 
                    {"status": status.value}
                )

        Enum for content types::\
        
            @fraise_enum
            class ContentType(Enum):
                ARTICLE = "article"
                VIDEO = "video"
                PODCAST = "podcast"
                INFOGRAPHIC = "infographic"

            @fraise_enum
            class ContentStatus(Enum):
                DRAFT = "draft"
                REVIEW = "review"
                PUBLISHED = "published"
                ARCHIVED = "archived"

            @fraise_type
            class Content:
                id: UUID
                title: str
                type: ContentType
                status: ContentStatus
                created_at: datetime

        Enum with integer values::\
        
            @fraise_enum
            class Priority(Enum):
                LOW = 1
                MEDIUM = 2
                HIGH = 3
                CRITICAL = 4

            @fraise_type
            class Task:
                id: UUID
                title: str
                priority: Priority
                
                @field(description="Priority as human-readable text")
                def priority_text(self) -> str:
                    return self.priority.name.lower()

        Enum for filtering and sorting::\
        
            @fraise_enum
            class SortDirection(Enum):
                ASC = "asc"
                DESC = "desc"

            @fraise_enum
            class UserSortField(Enum):
                NAME = "name"
                EMAIL = "email"
                CREATED_AT = "created_at"
                LAST_LOGIN = "last_login"

            @fraise_input
            class UserFilterInput:
                role: UserRole | None = None
                status: UserStatus | None = None
                sort_field: UserSortField = UserSortField.CREATED_AT
                sort_direction: SortDirection = SortDirection.DESC

            @query
            async def users(
                info, 
                filters: UserFilterInput | None = None
            ) -> list[User]:
                db = info.context["db"]
                where_clause = {}
                
                if filters:
                    if filters.role:
                        where_clause["role"] = filters.role.value
                    if filters.status:
                        where_clause["status"] = filters.status.value
                
                return await db.find(
                    "user_view",
                    where_clause,
                    order_by=f"{filters.sort_field.value} {filters.sort_direction.value}"
                )

        Enum for API versioning::\
        
            @fraise_enum
            class APIVersion(Enum):
                V1 = "v1"
                V2 = "v2"
                V3 = "v3"

            @fraise_input
            class APIRequestInput:
                version: APIVersion = APIVersion.V3
                data: dict[str, Any]

            @mutation
            async def process_request(
                info, 
                input: APIRequestInput
            ) -> APIResponse:
                # Handle different API versions
                if input.version == APIVersion.V1:
                    return await handle_v1_request(input.data)
                elif input.version == APIVersion.V2:
                    return await handle_v2_request(input.data)
                else:
                    return await handle_v3_request(input.data)

        Enum with custom descriptions (using docstrings)::\
        
            @fraise_enum
            class NotificationPreference(Enum):
                \"\"\"User notification preferences.\"\"\"
                EMAIL = "email"      # Send notifications via email
                SMS = "sms"          # Send notifications via SMS
                PUSH = "push"        # Send push notifications
                NONE = "none"        # Disable all notifications

            @fraise_type
            class UserSettings:
                user_id: UUID
                notifications: NotificationPreference
                
                @field(description="Check if user allows notifications")
                def notifications_enabled(self) -> bool:
                    return self.notifications != NotificationPreference.NONE

        Enum for database constraints::\
        
            @fraise_enum
            class Gender(Enum):
                MALE = "M"
                FEMALE = "F"
                OTHER = "O"
                PREFER_NOT_TO_SAY = "N"

            @fraise_type
            class UserProfile:
                user_id: UUID
                gender: Gender | None = None
                birth_date: date | None = None
                
                @field(description="User's age in years")
                def age(self) -> int | None:
                    if not self.birth_date:
                        return None
                    today = date.today()
                    return today.year - self.birth_date.year - (
                        (today.month, today.day) < 
                        (self.birth_date.month, self.birth_date.day)
                    )

    Notes:
        - Enum names (not values) are used as GraphQL enum values
        - Python enum values can be strings, integers, or other types
        - Enums provide type safety and IDE autocompletion
        - Use enums for predefined sets of values in your domain
        - Enum values are serialized consistently across API versions
        - Consider using enums for sorting, filtering, and status fields
        - Enum descriptions can be added via class docstrings
    """

    def wrap(cls: T) -> T:
        if not issubclass(cls, Enum):
            msg = f"@fraise_enum can only be used on Enum classes, not {cls.__name__}"
            raise TypeError(msg)

        # Import here to avoid circular imports
        from fraiseql.gql.schema_builder import SchemaRegistry

        # Create GraphQL enum type
        enum_values = {}
        for member in cls:
            # Use the enum member name as the GraphQL value name
            # GraphQL will serialize based on the GraphQLEnumValue's internal value
            enum_values[member.name] = GraphQLEnumValue(
                value=member.name,  # Store the GraphQL name
                description=getattr(member, "_description", None),
            )

        graphql_enum = GraphQLEnumType(
            name=cls.__name__,
            values=enum_values,
            description=cls.__doc__,
        )

        # Store the GraphQL type on the class for later retrieval
        cls.__graphql_type__ = graphql_enum

        # Register with schema
        SchemaRegistry.get_instance().register_enum(cls, graphql_enum)

        return cls

    return wrap if _cls is None else wrap(_cls)
