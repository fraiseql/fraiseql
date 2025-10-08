"""TurboRouter Configuration and Query Registration."""

from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry


def setup_turbo_router(app) -> TurboRegistry:
    """Configure and register queries with TurboRouter.

    Returns:
        TurboRegistry with pre-registered queries
    """
    registry = TurboRegistry(max_size=1000)

    # Register GetUser query
    user_query = TurboQuery(
        graphql_query="""
            query GetUser($id: Int!) {
                user(id: $id) {
                    id
                    name
                    email
                    created_at
                }
            }
        """,
        sql_template="""
            SELECT jsonb_build_object(
                'id', id,
                'name', name,
                'email', email,
                'createdAt', created_at
            ) as data
            FROM v_users
            WHERE id = %(id)s
        """,
        param_mapping={"id": "id"},
        operation_name="GetUser",
    )
    registry.register(user_query)

    # Register GetUsers query
    users_query = TurboQuery(
        graphql_query="""
            query GetUsers($limit: Int!, $offset: Int!) {
                users(limit: $limit, offset: $offset) {
                    id
                    name
                    email
                }
            }
        """,
        sql_template="""
            SELECT COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'email', email
                    )
                    ORDER BY id
                ),
                '[]'::jsonb
            ) as data
            FROM v_users
            LIMIT %(limit)s OFFSET %(offset)s
        """,
        param_mapping={"limit": "limit", "offset": "offset"},
        operation_name="GetUsers",
    )
    registry.register(users_query)

    # Register GetPost query
    post_query = TurboQuery(
        graphql_query="""
            query GetPost($id: Int!) {
                post(id: $id) {
                    id
                    title
                    content
                    published
                }
            }
        """,
        sql_template="""
            SELECT jsonb_build_object(
                'id', id,
                'title', title,
                'content', content,
                'published', published
            ) as data
            FROM v_posts
            WHERE id = %(id)s
        """,
        param_mapping={"id": "id"},
        operation_name="GetPost",
    )
    registry.register(post_query)

    # Register GetPosts query
    posts_query = TurboQuery(
        graphql_query="""
            query GetPosts($limit: Int!, $offset: Int!) {
                posts(limit: $limit, offset: $offset) {
                    id
                    title
                    published
                    created_at
                }
            }
        """,
        sql_template="""
            SELECT COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', id,
                        'title', title,
                        'published', published,
                        'createdAt', created_at
                    )
                    ORDER BY created_at DESC
                ),
                '[]'::jsonb
            ) as data
            FROM v_posts
            WHERE published = true
            LIMIT %(limit)s OFFSET %(offset)s
        """,
        param_mapping={"limit": "limit", "offset": "offset"},
        operation_name="GetPosts",
    )
    registry.register(posts_query)

    print(f"Registered {len(registry._queries)} queries with TurboRouter")
    return registry
