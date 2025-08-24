# GraphQL API Testing

End-to-end GraphQL testing verifies that your complete API works correctly, from HTTP requests through GraphQL parsing, query execution, and database operations to JSON responses.

## Test Client Setup

### HTTP Test Client

```python
# conftest.py
import pytest
from httpx import AsyncClient
from app import create_app  # Your FastAPI application factory

@pytest.fixture
async def test_client():
    """HTTP client for GraphQL API testing"""
    app = create_app()

    async with AsyncClient(app=app, base_url="http://test") as client:
        yield client

@pytest.fixture
async def authenticated_client(test_client, sample_user):
    """HTTP client with authentication headers"""
    # Login to get token
    login_response = await test_client.post("/graphql", json={
        "query": """
            mutation Login($email: String!, $password: String!) {
                login(email: $email, password: $password) {
                    token
                    user { id name }
                }
            }
        """,
        "variables": {
            "email": sample_user["email"],
            "password": "test_password"
        }
    })

    assert login_response.status_code == 200
    login_data = login_response.json()
    token = login_data["data"]["login"]["token"]

    # Add authorization header to client
    test_client.headers.update({"Authorization": f"Bearer {token}"})
    yield test_client

@pytest.fixture
async def graphql_client(test_client):
    """Helper client with GraphQL utilities"""
    class GraphQLClient:
        def __init__(self, client):
            self.client = client

        async def query(self, query: str, variables: dict = None):
            """Execute a GraphQL query"""
            response = await self.client.post("/graphql", json={
                "query": query,
                "variables": variables or {}
            })
            return response.json()

        async def mutate(self, mutation: str, variables: dict = None):
            """Execute a GraphQL mutation"""
            return await self.query(mutation, variables)

        def assert_success(self, response):
            """Assert GraphQL response has no errors"""
            assert "errors" not in response, f"GraphQL errors: {response.get('errors')}"
            assert "data" in response

        def assert_error(self, response, expected_code: str = None):
            """Assert GraphQL response has errors with native error arrays"""
            assert "errors" in response
            if expected_code:
                codes = [err.get("extensions", {}).get("code") for err in response["errors"]]
                assert expected_code in codes

    return GraphQLClient(test_client)
```

## Query Testing

### Basic Query Tests

```python
# test_graphql_queries.py
import pytest

@pytest.mark.asyncio
class TestUserQueries:
    async def test_get_users_query(self, graphql_client, sample_users):
        """Test basic users query"""
        query = """
            query GetUsers {
                users {
                    id
                    name
                    email
                    createdAt
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_success(response)

        users = response["data"]["users"]
        assert isinstance(users, list)
        assert len(users) >= 3  # From sample_users fixture

        # Verify user structure
        for user in users:
            assert "id" in user
            assert "name" in user
            assert "email" in user
            assert "createdAt" in user
            assert "@" in user["email"]  # Basic email validation

    async def test_get_user_by_id_query(self, graphql_client, sample_user):
        """Test single user query by ID"""
        query = """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                    email
                    posts {
                        id
                        title
                    }
                }
            }
        """

        response = await graphql_client.query(query, {
            "id": sample_user["id"]
        })
        graphql_client.assert_success(response)

        user = response["data"]["user"]
        assert user is not None
        assert user["id"] == sample_user["id"]
        assert user["name"] == sample_user["name"]
        assert "posts" in user  # Nested field

    async def test_get_user_not_found(self, graphql_client):
        """Test user query with non-existent ID"""
        query = """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                }
            }
        """

        response = await graphql_client.query(query, {
            "id": "00000000-0000-0000-0000-000000000000"  # Non-existent ID
        })
        graphql_client.assert_success(response)

        # Should return null for non-existent user
        assert response["data"]["user"] is None

    async def test_users_with_filters(self, graphql_client, sample_users):
        """Test users query with filters"""
        query = """
            query GetFilteredUsers($nameContains: String, $limit: Int) {
                users(nameContains: $nameContains, limit: $limit) {
                    id
                    name
                    email
                }
            }
        """

        response = await graphql_client.query(query, {
            "nameContains": "User 1",
            "limit": 1
        })
        graphql_client.assert_success(response)

        users = response["data"]["users"]
        assert len(users) == 1
        assert "User 1" in users[0]["name"]

    async def test_nested_query_posts_and_comments(self, graphql_client, sample_user_with_posts):
        """Test complex nested query"""
        query = """
            query GetUserWithPosts($userId: ID!) {
                user(id: $userId) {
                    id
                    name
                    posts {
                        id
                        title
                        content
                        status
                        comments {
                            id
                            content
                            author {
                                id
                                name
                            }
                        }
                        createdAt
                    }
                }
            }
        """

        response = await graphql_client.query(query, {
            "userId": sample_user_with_posts["id"]
        })
        graphql_client.assert_success(response)

        user = response["data"]["user"]
        assert user is not None
        assert len(user["posts"]) > 0

        # Verify post structure
        for post in user["posts"]:
            assert all(field in post for field in ["id", "title", "content", "status", "createdAt"])
            assert "comments" in post
```

### Query Error Handling

```python
# test_graphql_query_errors.py
import pytest

@pytest.mark.asyncio
class TestQueryErrorHandling:
    async def test_invalid_query_syntax(self, test_client):
        """Test handling of invalid GraphQL syntax"""
        response = await test_client.post("/graphql", json={
            "query": "query { users { invalid syntax here"  # Missing closing braces
        })

        assert response.status_code == 400
        data = response.json()
        assert "errors" in data
        assert "syntax" in data["errors"][0]["message"].lower()

    async def test_unknown_field_error(self, graphql_client):
        """Test querying unknown fields"""
        query = """
            query {
                users {
                    id
                    name
                    nonExistentField  # This field doesn't exist
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_error(response)

        error_message = response["errors"][0]["message"]
        assert "nonExistentField" in error_message

    async def test_invalid_argument_type(self, graphql_client):
        """Test invalid argument types"""
        query = """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                }
            }
        """

        response = await graphql_client.query(query, {
            "id": 12345  # Should be string, not integer
        })
        graphql_client.assert_error(response)

    async def test_missing_required_arguments(self, graphql_client):
        """Test missing required arguments"""
        query = """
            query GetUser($id: ID!) {
                user(id: $id) {  # $id variable not provided
                    id
                    name
                }
            }
        """

        response = await graphql_client.query(query)  # Missing variables
        graphql_client.assert_error(response)

        error_message = response["errors"][0]["message"]
        assert "variable" in error_message.lower()

    async def test_query_complexity_limit(self, graphql_client):
        """Test query complexity validation"""
        # Very deep nested query that should be rejected
        query = """
            query DeepQuery {
                users {
                    posts {
                        comments {
                            author {
                                posts {
                                    comments {
                                        author {
                                            posts {
                                                id
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_error(response, "QUERY_TOO_COMPLEX")

    async def test_query_depth_limit(self, graphql_client):
        """Test query depth validation"""
        # Query exceeding maximum depth
        query = """
            query TooDeep {
                users {
                    posts {
                        comments {
                            author {
                                posts {
                                    comments {
                                        author {
                                            posts {
                                                comments {
                                                    author {
                                                        id
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_error(response, "QUERY_TOO_DEEP")
```

## Mutation Testing

### Basic Mutation Tests

```python
# test_graphql_mutations.py
import pytest

@pytest.mark.asyncio
class TestUserMutations:
    async def test_create_user_mutation(self, graphql_client):
        """Test successful user creation"""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    id
                    name
                    email
                    createdAt
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "name": "New GraphQL User",
                "email": "graphql@example.com",
                "password": "secure_password"
            }
        })
        graphql_client.assert_success(response)

        user = response["data"]["createUser"]
        assert user["name"] == "New GraphQL User"
        assert user["email"] == "graphql@example.com"
        assert "id" in user
        assert "createdAt" in user

    async def test_create_user_duplicate_email(self, graphql_client, sample_user):
        """Test user creation with duplicate email"""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    id
                    name
                    email
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "name": "Duplicate User",
                "email": sample_user["email"],  # Existing email
                "password": "password123"
            }
        })
        graphql_client.assert_error(response, "DUPLICATE_EMAIL")

        error = response["errors"][0]
        assert "email already exists" in error["message"].lower()

    async def test_update_user_mutation(self, graphql_client, sample_user):
        """Test user update mutation"""
        mutation = """
            mutation UpdateUser($id: ID!, $input: UpdateUserInput!) {
                updateUser(id: $id, input: $input) {
                    id
                    name
                    email
                    updatedAt
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "id": sample_user["id"],
            "input": {
                "name": "Updated Name"
            }
        })
        graphql_client.assert_success(response)

        user = response["data"]["updateUser"]
        assert user["id"] == sample_user["id"]
        assert user["name"] == "Updated Name"
        assert user["email"] == sample_user["email"]  # Unchanged

    async def test_delete_user_mutation(self, graphql_client, sample_user):
        """Test user deletion"""
        mutation = """
            mutation DeleteUser($id: ID!) {
                deleteUser(id: $id)
            }
        """

        response = await graphql_client.mutate(mutation, {
            "id": sample_user["id"]
        })
        graphql_client.assert_success(response)

        assert response["data"]["deleteUser"] is True

        # Verify user was deleted
        query = """
            query GetUser($id: ID!) {
                user(id: $id) { id }
            }
        """

        check_response = await graphql_client.query(query, {
            "id": sample_user["id"]
        })
        graphql_client.assert_success(check_response)
        assert check_response["data"]["user"] is None

@pytest.mark.asyncio
class TestPostMutations:
    async def test_create_post_mutation(self, authenticated_client, graphql_client, sample_user):
        """Test creating a post with authentication"""
        # Note: Using authenticated_client fixture
        graphql_client.client = authenticated_client

        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    id
                    title
                    content
                    status
                    author {
                        id
                        name
                    }
                    createdAt
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "title": "My New Post",
                "content": "This is the content of my new post.",
                "status": "PUBLISHED"
            }
        })
        graphql_client.assert_success(response)

        post = response["data"]["createPost"]
        assert post["title"] == "My New Post"
        assert post["content"] == "This is the content of my new post."
        assert post["status"] == "PUBLISHED"
        assert post["author"]["id"] == sample_user["id"]

    async def test_create_post_without_auth(self, graphql_client):
        """Test creating post without authentication"""
        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    id
                    title
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "title": "Unauthorized Post",
                "content": "This should fail"
            }
        })
        graphql_client.assert_error(response, "UNAUTHENTICATED")
```

### Input Validation Testing

```python
# test_mutation_input_validation.py
import pytest

@pytest.mark.asyncio
class TestMutationInputValidation:
    async def test_create_user_empty_name(self, graphql_client):
        """Test user creation with empty name"""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    id
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "name": "",  # Empty name
                "email": "empty@example.com",
                "password": "password123"
            }
        })
        graphql_client.assert_error(response, "VALIDATION_ERROR")

        error = response["errors"][0]
        assert "name" in error["message"].lower()
        assert "empty" in error["message"].lower()

    async def test_create_user_invalid_email(self, graphql_client):
        """Test user creation with invalid email format"""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    id
                }
            }
        """

        invalid_emails = [
            "not-an-email",
            "@example.com",
            "test@",
            "spaces in@email.com"
        ]

        for email in invalid_emails:
            response = await graphql_client.mutate(mutation, {
                "input": {
                    "name": "Test User",
                    "email": email,
                    "password": "password123"
                }
            })
            graphql_client.assert_error(response, "VALIDATION_ERROR")

    async def test_create_user_weak_password(self, graphql_client):
        """Test user creation with weak password"""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    id
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "name": "Test User",
                "email": "test@example.com",
                "password": "123"  # Too short
            }
        })
        graphql_client.assert_error(response, "VALIDATION_ERROR")

        error = response["errors"][0]
        assert "password" in error["message"].lower()

    async def test_create_post_invalid_status(self, authenticated_client, graphql_client):
        """Test post creation with invalid status"""
        graphql_client.client = authenticated_client

        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    id
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "title": "Test Post",
                "content": "Content here",
                "status": "INVALID_STATUS"  # Not a valid enum value
            }
        })
        graphql_client.assert_error(response)

        error = response["errors"][0]
        assert "status" in error["message"].lower() or "enum" in error["message"].lower()
```

## Schema Testing

### Introspection Tests

```python
# test_graphql_schema.py
import pytest
from graphql import build_schema, validate, parse

@pytest.mark.asyncio
class TestGraphQLSchema:
    async def test_schema_introspection(self, graphql_client):
        """Test GraphQL schema introspection"""
        query = """
            query IntrospectionQuery {
                __schema {
                    types {
                        name
                        kind
                        fields {
                            name
                            type {
                                name
                            }
                        }
                    }
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_success(response)

        schema = response["data"]["__schema"]
        type_names = [t["name"] for t in schema["types"]]

        # Verify expected types exist
        assert "User" in type_names
        assert "Post" in type_names
        assert "Query" in type_names
        assert "Mutation" in type_names

    async def test_user_type_fields(self, graphql_client):
        """Test User type has expected fields"""
        query = """
            query {
                __type(name: "User") {
                    name
                    fields {
                        name
                        type {
                            name
                        }
                    }
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_success(response)

        user_type = response["data"]["__type"]
        field_names = [f["name"] for f in user_type["fields"]]

        expected_fields = ["id", "name", "email", "createdAt", "updatedAt", "posts"]
        for field in expected_fields:
            assert field in field_names

    async def test_query_type_fields(self, graphql_client):
        """Test Query type has expected fields"""
        query = """
            query {
                __type(name: "Query") {
                    fields {
                        name
                        args {
                            name
                            type {
                                name
                            }
                        }
                    }
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_success(response)

        query_type = response["data"]["__type"]
        field_names = [f["name"] for f in query_type["fields"]]

        expected_queries = ["users", "user", "posts", "post"]
        for query_field in expected_queries:
            assert query_field in field_names

    async def test_mutation_type_fields(self, graphql_client):
        """Test Mutation type has expected fields"""
        query = """
            query {
                __type(name: "Mutation") {
                    fields {
                        name
                        args {
                            name
                            type {
                                name
                            }
                        }
                    }
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_success(response)

        mutation_type = response["data"]["__type"]
        field_names = [f["name"] for f in mutation_type["fields"]]

        expected_mutations = ["createUser", "updateUser", "deleteUser", "createPost"]
        for mutation_field in expected_mutations:
            assert mutation_field in field_names

    def test_schema_validity_offline(self):
        """Test schema validity without HTTP requests"""
        from app.schema import get_schema  # Your schema factory

        schema = get_schema()

        # Test basic query parsing
        query = parse("""
            query {
                users {
                    id
                    name
                    posts {
                        id
                        title
                    }
                }
            }
        """)

        # Validate query against schema
        errors = validate(schema, query)
        assert len(errors) == 0, f"Schema validation errors: {errors}"
```

## Authentication and Authorization Testing

```python
# test_graphql_auth.py
import pytest

@pytest.mark.asyncio
class TestGraphQLAuthentication:
    async def test_public_queries_no_auth_required(self, graphql_client):
        """Test public queries work without authentication"""
        query = """
            query {
                users {
                    id
                    name
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_success(response)

    async def test_protected_query_requires_auth(self, graphql_client):
        """Test protected queries require authentication"""
        query = """
            query {
                myProfile {
                    id
                    email
                    privateNotes
                }
            }
        """

        response = await graphql_client.query(query)
        graphql_client.assert_error(response, "UNAUTHENTICATED")

    async def test_protected_mutation_requires_auth(self, graphql_client):
        """Test protected mutations require authentication"""
        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    id
                }
            }
        """

        response = await graphql_client.mutate(mutation, {
            "input": {
                "title": "Protected Post",
                "content": "Should require auth"
            }
        })
        graphql_client.assert_error(response, "UNAUTHENTICATED")

    async def test_field_level_authorization(self, authenticated_client, graphql_client, sample_users):
        """Test field-level authorization"""
        graphql_client.client = authenticated_client

        # Query that includes both public and private fields
        query = """
            query GetUsers {
                users {
                    id
                    name
                    email
                    # This field should only be visible to admins
                    internalNotes
                }
            }
        """

        response = await graphql_client.query(query)

        # Should succeed but internalNotes should be null for non-admin users
        graphql_client.assert_success(response)
        users = response["data"]["users"]

        for user in users:
            # Non-admin users shouldn't see internal notes
            assert user["internalNotes"] is None

    async def test_resource_ownership_authorization(self, authenticated_client, graphql_client):
        """Test users can only modify their own resources"""
        graphql_client.client = authenticated_client

        # Try to update another user's post
        mutation = """
            mutation UpdatePost($id: ID!, $input: UpdatePostInput!) {
                updatePost(id: $id, input: $input) {
                    id
                    title
                }
            }
        """

        # Assume we have another user's post ID
        other_user_post_id = "other-user-post-id"

        response = await graphql_client.mutate(mutation, {
            "id": other_user_post_id,
            "input": {
                "title": "Hacked Title"
            }
        })
        graphql_client.assert_error(response, "FORBIDDEN")
```

## Subscription Testing (WebSocket)

```python
# test_graphql_subscriptions.py
import pytest
import asyncio
import websockets
import json

@pytest.mark.asyncio
class TestGraphQLSubscriptions:
    async def test_post_created_subscription(self, test_db, sample_user):
        """Test subscription to post creation events"""
        # Connect to WebSocket endpoint
        uri = "ws://localhost:8000/graphql"

        async with websockets.connect(uri, subprotocols=["graphql-ws"]) as websocket:
            # Initialize connection
            await websocket.send(json.dumps({
                "type": "connection_init"
            }))

            init_response = await websocket.recv()
            assert json.loads(init_response)["type"] == "connection_ack"

            # Start subscription
            subscription = """
                subscription PostCreated {
                    postCreated {
                        id
                        title
                        author {
                            name
                        }
                    }
                }
            """

            await websocket.send(json.dumps({
                "id": "1",
                "type": "start",
                "payload": {
                    "query": subscription
                }
            }))

            # Create a post in another task to trigger subscription
            async def create_post():
                await asyncio.sleep(0.1)  # Small delay
                await test_db.call_function(
                    "fn_create_post",
                    p_title="Subscription Test Post",
                    p_content="This should trigger subscription",
                    p_author_id=sample_user["id"]
                )

            create_task = asyncio.create_task(create_post())

            # Wait for subscription event
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=5.0)
                data = json.loads(response)

                assert data["type"] == "data"
                assert data["id"] == "1"

                post_data = data["payload"]["data"]["postCreated"]
                assert post_data["title"] == "Subscription Test Post"
                assert post_data["author"]["name"] == sample_user["name"]

            except asyncio.TimeoutError:
                pytest.fail("Subscription did not receive event within timeout")
            finally:
                await create_task

    async def test_subscription_authentication(self):
        """Test subscription requires authentication when needed"""
        uri = "ws://localhost:8000/graphql"

        async with websockets.connect(uri, subprotocols=["graphql-ws"]) as websocket:
            # Initialize without auth token
            await websocket.send(json.dumps({
                "type": "connection_init"
            }))

            await websocket.recv()  # connection_ack

            # Try to subscribe to protected subscription
            subscription = """
                subscription MyNotifications {
                    userNotifications {
                        id
                        message
                    }
                }
            """

            await websocket.send(json.dumps({
                "id": "1",
                "type": "start",
                "payload": {
                    "query": subscription
                }
            }))

            # Should receive error
            response = await websocket.recv()
            data = json.loads(response)

            assert data["type"] == "error"
            assert "authentication" in data["payload"]["message"].lower()
```

## Performance and Load Testing

```python
# test_graphql_performance.py
import pytest
import asyncio
import time
import statistics

@pytest.mark.asyncio
class TestGraphQLPerformance:
    async def test_query_response_time(self, graphql_client, sample_users):
        """Test GraphQL query response times"""
        query = """
            query GetUsers {
                users {
                    id
                    name
                    email
                    posts {
                        id
                        title
                    }
                }
            }
        """

        # Measure response times
        times = []
        for _ in range(20):
            start = time.perf_counter()
            response = await graphql_client.query(query)
            elapsed = time.perf_counter() - start
            times.append(elapsed)

            graphql_client.assert_success(response)

        # Calculate statistics
        avg_time = statistics.mean(times)
        p95_time = statistics.quantiles(times, n=20)[18]  # 95th percentile

        # Performance assertions
        assert avg_time < 0.5, f"Average response time too slow: {avg_time:.3f}s"
        assert p95_time < 1.0, f"95th percentile too slow: {p95_time:.3f}s"

        print(f"Query performance - Avg: {avg_time*1000:.2f}ms, P95: {p95_time*1000:.2f}ms")

    async def test_concurrent_queries(self, graphql_client):
        """Test handling of concurrent GraphQL queries"""
        query = """
            query GetUsers($limit: Int) {
                users(limit: $limit) {
                    id
                    name
                }
            }
        """

        # Create 50 concurrent queries
        tasks = []
        for i in range(50):
            task = graphql_client.query(query, {"limit": 10})
            tasks.append(task)

        start_time = time.perf_counter()
        responses = await asyncio.gather(*tasks)
        elapsed = time.perf_counter() - start_time

        # Verify all queries succeeded
        for response in responses:
            graphql_client.assert_success(response)
            assert len(response["data"]["users"]) <= 10

        # Performance check
        assert elapsed < 5.0, f"Concurrent queries took too long: {elapsed:.2f}s"

        queries_per_second = 50 / elapsed
        print(f"Handled {queries_per_second:.1f} queries/second")

    async def test_n_plus_one_detection(self, graphql_client, sample_users_with_posts):
        """Test that N+1 queries are properly handled"""
        # This query could cause N+1 if not properly optimized
        query = """
            query UsersWithPosts {
                users {
                    id
                    name
                    posts {
                        id
                        title
                        comments {
                            id
                            content
                        }
                    }
                }
            }
        """

        # Enable query logging to detect N+1
        import logging
        logging.getLogger("fraiseql.repository").setLevel(logging.DEBUG)

        start_time = time.perf_counter()
        response = await graphql_client.query(query)
        elapsed = time.perf_counter() - start_time

        graphql_client.assert_success(response)

        # With proper DataLoader implementation, this should be fast
        # even with many users and posts
        assert elapsed < 2.0, f"Query with nested data too slow: {elapsed:.2f}s"

        users = response["data"]["users"]
        assert len(users) > 0

        # Verify data structure
        for user in users:
            assert "posts" in user
            for post in user["posts"]:
                assert "comments" in post
```

## Running GraphQL Tests

### Command Line Examples

```bash
# Run all GraphQL tests
pytest tests/graphql/ -v

# Run with HTTP client logs
pytest tests/graphql/ -v -s --log-cli-level=DEBUG

# Test specific GraphQL operations
pytest tests/graphql/test_queries.py -v

# Run performance tests separately
pytest tests/graphql/test_performance.py -v -m slow

# Test with coverage
pytest tests/graphql/ --cov=app --cov-report=html

# Run against different environments
TEST_BASE_URL=http://staging.example.com pytest tests/graphql/ -v
```

### Test Configuration

```python
# pytest.ini
[tool:pytest]
markers =
    slow: marks tests as slow (deselect with '-m "not slow"')
    websocket: marks tests requiring WebSocket support
    auth: marks tests requiring authentication

# Environment-specific test settings
env_files = [
    .env.test
    .env.test.local
]
```

GraphQL API tests provide the highest level of confidence that your complete system works correctly. They test the entire request/response cycle and catch integration issues that unit and integration tests might miss. However, they're typically slower and more complex to set up and maintain.
