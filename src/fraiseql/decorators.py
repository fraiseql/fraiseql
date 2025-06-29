"""Additional decorators for FraiseQL."""

import asyncio
import time
from collections.abc import Callable
from typing import Any, TypeVar, overload

from fraiseql.gql.schema_builder import SchemaRegistry

F = TypeVar("F", bound=Callable[..., Any])


@overload
def query(fn: F) -> F: ...


@overload
def query() -> Callable[[F], F]: ...


def query(fn: F | None = None) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL query.

    This decorator automatically registers the function with the GraphQL schema,
    eliminating the need to manually pass queries to create_fraiseql_app.

    Args:
        fn: The query function to decorate (when used without parentheses)

    Returns:
        The decorated function with GraphQL query metadata

    Examples:
        Basic query with database access::

            @fraiseql.query
            async def get_user(info, id: UUID) -> User:
                db = info.context["db"]
                return await db.find_one("user_view", {"id": id})

        Query with multiple parameters::

            @fraiseql.query
            async def search_users(
                info,
                name_filter: str | None = None,
                limit: int = 10
            ) -> list[User]:
                db = info.context["db"]
                filters = {}
                if name_filter:
                    filters["name__icontains"] = name_filter
                return await db.find("user_view", filters, limit=limit)

        Query with authentication and authorization::

            @fraiseql.query
            async def get_my_profile(info) -> User:
                user_context = info.context["user"]
                if not user_context:
                    raise GraphQLError("Authentication required")

                db = info.context["db"]
                return await db.find_one("user_view", {"id": user_context.user_id})

        Query with error handling::

            @fraiseql.query
            async def get_post(info, id: UUID) -> Post | None:
                try:
                    db = info.context["db"]
                    return await db.find_one("post_view", {"id": id})
                except Exception as e:
                    logger.error(f"Failed to fetch post {id}: {e}")
                    return None

        Query using custom repository methods::

            @fraiseql.query
            async def get_user_stats(info, user_id: UUID) -> UserStats:
                db = info.context["db"]
                # Custom SQL query for complex aggregations
                result = await db.execute_raw(
                    "SELECT count(*) as post_count FROM posts WHERE user_id = $1",
                    user_id
                )
                return UserStats(post_count=result[0]["post_count"])

    Notes:
        - Functions decorated with @query are automatically discovered
        - The first parameter is always 'info' (GraphQL resolver info)
        - Return type annotation is used for GraphQL schema generation
        - Use async/await for database operations
        - Access database via info.context["db"]
        - Access user context via info.context["user"] if authentication is enabled
    """

    def decorator(func: F) -> F:
        # Register with schema
        registry = SchemaRegistry.get_instance()
        registry.register_query(func)
        return func

    if fn is None:
        return decorator
    return decorator(fn)


@overload
def subscription(fn: F) -> F: ...


@overload
def subscription() -> Callable[[F], F]: ...


def subscription(fn: F | None = None) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL subscription.

    This decorator automatically registers the function with the GraphQL schema
    for real-time subscriptions. Subscriptions must be async generator functions
    that yield values over time.

    Args:
        fn: The subscription function to decorate (when used without parentheses)

    Returns:
        The decorated async generator function with GraphQL subscription metadata

    Examples:
        Basic subscription for real-time updates::

            @fraiseql.subscription
            async def on_post_created(info) -> AsyncGenerator[Post, None]:
                # Subscribe to post creation events
                async for post in post_event_stream():
                    yield post

        Filtered subscription with parameters::

            @fraiseql.subscription
            async def on_user_posts(
                info,
                user_id: UUID
            ) -> AsyncGenerator[Post, None]:
                # Only yield posts from specific user
                async for post in post_event_stream():
                    if post.user_id == user_id:
                        yield post

        Subscription with authentication::

            @fraiseql.subscription
            async def on_private_messages(
                info
            ) -> AsyncGenerator[Message, None]:
                user_context = info.context.get("user")
                if not user_context:
                    raise GraphQLError("Authentication required")

                async for message in message_stream():
                    # Only yield messages for authenticated user
                    if message.recipient_id == user_context.user_id:
                        yield message

        Subscription with database polling::

            @fraiseql.subscription
            async def on_task_updates(
                info,
                project_id: UUID
            ) -> AsyncGenerator[Task, None]:
                db = info.context["db"]
                last_check = datetime.utcnow()

                while True:
                    # Poll for new/updated tasks
                    updated_tasks = await db.find(
                        "task_view",
                        {
                            "project_id": project_id,
                            "updated_at__gt": last_check
                        }
                    )

                    for task in updated_tasks:
                        yield task

                    last_check = datetime.utcnow()
                    await asyncio.sleep(1)  # Poll every second

        Subscription with error handling and cleanup::

            @fraiseql.subscription
            async def on_notifications(
                info
            ) -> AsyncGenerator[Notification, None]:
                connection = None
                try:
                    connection = await connect_to_message_broker()
                    async for notification in connection.subscribe("notifications"):
                        yield notification
                except Exception as e:
                    logger.error(f"Subscription error: {e}")
                    raise
                finally:
                    if connection:
                        await connection.close()

    Notes:
        - Subscription functions MUST be async generators (use 'async def' and 'yield')
        - Return type must be AsyncGenerator[YieldType, None]
        - The first parameter is always 'info' (GraphQL resolver info)
        - Use WebSocket transport for GraphQL subscriptions
        - Consider rate limiting and authentication for production use
        - Handle connection cleanup in finally blocks
        - Use asyncio.sleep() for polling-based subscriptions
    """

    def decorator(func: F) -> F:
        # Register with schema
        registry = SchemaRegistry.get_instance()
        registry.register_subscription(func)
        return func

    if fn is None:
        return decorator
    return decorator(fn)


@overload
def field(
    method: F,
    *,
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
) -> F: ...


@overload
def field(
    *,
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
) -> Callable[[F], F]: ...


def field(
    method: F | None = None,
    *,
    resolver: Callable[..., Any] | None = None,
    description: str | None = None,
) -> F | Callable[[F], F]:
    """Decorator to mark a method as a GraphQL field with optional resolver.

    This decorator should be applied to methods of @fraise_type decorated classes.
    It allows defining custom field resolvers, adding field descriptions, and
    implementing computed fields with complex logic.

    Args:
        method: The method to decorate (when used without parentheses)
        resolver: Optional custom resolver function to override default behavior
        description: Field description that appears in GraphQL schema documentation

    Returns:
        Decorated method with GraphQL field metadata and N+1 query detection

    Examples:
        Computed field with description::\
        
            @fraise_type
            class User:
                first_name: str
                last_name: str

                @field(description="User's full display name")
                def display_name(self) -> str:
                    return f"{self.first_name} {self.last_name}"
        
        Async field with database access::\
        
            @fraise_type
            class User:
                id: UUID
                
                @field(description="Posts authored by this user")
                async def posts(self, info) -> list[Post]:
                    db = info.context["db"]
                    return await db.find("post_view", {"user_id": self.id})
        
        Field with custom resolver function::\
        
            async def fetch_user_posts_optimized(root, info):
                # Custom resolver with optimized loading
                db = info.context["db"]
                # Use DataLoader or batch loading here
                return await batch_load_posts([root.id])
            
            @fraise_type
            class User:
                id: UUID
                
                @field(
                    resolver=fetch_user_posts_optimized,
                    description="Posts with optimized loading"
                )
                async def posts(self) -> list[Post]:
                    # This method signature defines the GraphQL schema
                    # but fetch_user_posts_optimized handles the actual resolution
                    pass
        
        Field with parameters::\
        
            @fraise_type
            class User:
                id: UUID
                
                @field(description="User's posts with optional filtering")
                async def posts(
                    self, 
                    info, 
                    published_only: bool = False,
                    limit: int = 10
                ) -> list[Post]:
                    db = info.context["db"]
                    filters = {"user_id": self.id}
                    if published_only:
                        filters["status"] = "published"
                    return await db.find("post_view", filters, limit=limit)
        
        Field with authentication and authorization::\
        
            @fraise_type
            class User:
                id: UUID
                
                @field(description="Private user settings (owner only)")
                async def settings(self, info) -> UserSettings | None:
                    user_context = info.context.get("user")
                    if not user_context or user_context.user_id != self.id:
                        return None  # Don't expose private data
                    
                    db = info.context["db"]
                    return await db.find_one("user_settings_view", {"user_id": self.id})
        
        Field with error handling::\
        
            @fraise_type
            class User:
                id: UUID
                
                @field(description="User's profile image URL")
                async def avatar_url(self, info) -> str | None:
                    try:
                        storage = info.context["storage"]
                        return await storage.get_user_avatar_url(self.id)
                    except StorageError:
                        logger.warning(f"Failed to get avatar for user {self.id}")
                        return None
        
        Field with caching::\
        
            @fraise_type
            class Post:
                id: UUID
                
                @field(description="Number of likes (cached)")
                async def like_count(self, info) -> int:
                    cache = info.context.get("cache")
                    cache_key = f"post:{self.id}:likes"
                    
                    # Try cache first
                    if cache:
                        cached_count = await cache.get(cache_key)
                        if cached_count is not None:
                            return int(cached_count)
                    
                    # Fallback to database
                    db = info.context["db"]
                    result = await db.execute_raw(
                        "SELECT count(*) FROM likes WHERE post_id = $1",
                        self.id
                    )
                    count = result[0]["count"]
                    
                    # Cache for 5 minutes
                    if cache:
                        await cache.set(cache_key, count, ttl=300)
                    
                    return count

    Notes:
        - Fields are automatically included in GraphQL schema generation
        - Use 'info' parameter to access GraphQL context (database, user, etc.)
        - Async fields support database queries and external API calls
        - Custom resolvers can implement optimized data loading patterns
        - N+1 query detection is automatically enabled for performance monitoring
        - Return None from fields to indicate null values in GraphQL
        - Use type annotations for automatic GraphQL type generation
    """

    def decorator(func: F) -> F:
        # Determine if the function is async
        is_async = asyncio.iscoroutinefunction(func)

        if is_async:

            async def async_wrapped_resolver(root, info, *args, **kwargs):
                # Check if N+1 detector is available in context
                detector = None
                if info and hasattr(info, "context") and info.context:
                    detector = getattr(info.context, "get", lambda x: None)("n1_detector")
                if detector and detector.enabled:
                    start_time = time.time()
                    try:
                        # Call the original method - if it's a bound method, use root as self
                        if hasattr(func, "__self__"):
                            result = await func(info, *args, **kwargs)
                        else:
                            result = await func(root, info, *args, **kwargs)
                        execution_time = time.time() - start_time
                        # Track field resolution without blocking
                        # Using create_task is safe here as detector manages its own lifecycle
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        # Add error handler to prevent unhandled exceptions
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        return result  # noqa: TRY300
                    except Exception:
                        execution_time = time.time() - start_time
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        raise
                # Call the original method - if it's a bound method, use root as self
                elif hasattr(func, "__self__"):
                    return await func(info, *args, **kwargs)
                else:
                    return await func(root, info, *args, **kwargs)

            wrapped_func = async_wrapped_resolver

        else:

            def sync_wrapped_resolver(root, info, *args, **kwargs):
                # Check if N+1 detector is available in context
                detector = None
                if info and hasattr(info, "context") and info.context:
                    detector = getattr(info.context, "get", lambda x: None)("n1_detector")
                if detector and detector.enabled:
                    start_time = time.time()
                    try:
                        # Call the original method - if it's a bound method, use root as self
                        if hasattr(func, "__self__"):
                            result = func(info, *args, **kwargs)
                        else:
                            result = func(root, info, *args, **kwargs)
                        execution_time = time.time() - start_time
                        # Track field resolution without blocking
                        # Using create_task is safe here as detector manages its own lifecycle
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        # Add error handler to prevent unhandled exceptions
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        return result  # noqa: TRY300
                    except Exception:
                        execution_time = time.time() - start_time
                        task = asyncio.create_task(
                            detector.track_field_resolution(info, info.field_name, execution_time),
                        )
                        task.add_done_callback(lambda t: t.exception() if t.done() else None)
                        raise
                # Call the original method - if it's a bound method, use root as self
                elif hasattr(func, "__self__"):
                    return func(info, *args, **kwargs)
                else:
                    return func(root, info, *args, **kwargs)

            wrapped_func = sync_wrapped_resolver

        # Copy over the metadata
        wrapped_func.__fraiseql_field__ = True
        wrapped_func.__fraiseql_field_resolver__ = resolver or wrapped_func
        wrapped_func.__fraiseql_field_description__ = description
        wrapped_func.__name__ = func.__name__
        wrapped_func.__doc__ = func.__doc__

        # Store the original function for field authorization
        wrapped_func.__fraiseql_original_func__ = func

        # Copy type annotations
        if hasattr(func, "__annotations__"):
            wrapped_func.__annotations__ = func.__annotations__.copy()

        return wrapped_func  # type: ignore[return-value]

    if method is None:
        return decorator
    return decorator(method)
