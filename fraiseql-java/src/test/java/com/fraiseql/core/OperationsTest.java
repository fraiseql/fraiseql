package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Comprehensive tests for FraiseQL GraphQL operations.
 * Tests query, mutation, and subscription builders and registration.
 */
@DisplayName("GraphQL Operations")
public class OperationsTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    // =========================================================================
    // QUERY TESTS
    // =========================================================================

    @Test
    @DisplayName("Create simple query with string return type")
    void testSimpleQueryWithStringReturnType() {
        FraiseQL.query("users")
            .returnType("User")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("users");

        assertTrue(query.isPresent());
        assertEquals("users", query.get().name);
        assertEquals("User", query.get().returnType);
    }

    @Test
    @DisplayName("Create query with class return type")
    void testQueryWithClassReturnType() {
        FraiseQL.registerType(User.class);

        FraiseQL.query("getUser")
            .returnType(User.class)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("getUser");

        assertTrue(query.isPresent());
        assertEquals("User", query.get().returnType);
    }

    @Test
    @DisplayName("Create query that returns array")
    void testQueryReturnsArray() {
        FraiseQL.query("allUsers")
            .returnType("User")
            .returnsArray(true)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("allUsers");

        assertTrue(query.isPresent());
        assertEquals("[User]", query.get().returnType);
    }

    @Test
    @DisplayName("Create query with single argument")
    void testQueryWithSingleArgument() {
        FraiseQL.query("userById")
            .returnType("User")
            .arg("id", "Int")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("userById");

        assertTrue(query.isPresent());
        assertEquals(1, query.get().arguments.size());
        assertTrue(query.get().arguments.containsKey("id"));
        assertEquals("Int", query.get().arguments.get("id"));
    }

    @Test
    @DisplayName("Create query with multiple arguments")
    void testQueryWithMultipleArguments() {
        FraiseQL.query("searchUsers")
            .returnType("User")
            .returnsArray(true)
            .arg("name", "String")
            .arg("email", "String")
            .arg("limit", "Int")
            .arg("offset", "Int")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("searchUsers");

        assertTrue(query.isPresent());
        assertEquals(4, query.get().arguments.size());
        assertTrue(query.get().arguments.containsKey("name"));
        assertTrue(query.get().arguments.containsKey("email"));
        assertTrue(query.get().arguments.containsKey("limit"));
        assertTrue(query.get().arguments.containsKey("offset"));
    }

    @Test
    @DisplayName("Create query with description")
    void testQueryWithDescription() {
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .description("Get all users in the system")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("users");

        assertTrue(query.isPresent());
        assertEquals("Get all users in the system", query.get().description);
    }

    @Test
    @DisplayName("Builder method chaining works correctly")
    void testQueryBuilderChaining() {
        FraiseQL.query("complexQuery")
            .returnType("User")
            .returnsArray(true)
            .arg("id", "Int")
            .arg("name", "String")
            .description("Complex query example")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("complexQuery");

        assertTrue(query.isPresent());
        assertEquals("[User]", query.get().returnType);
        assertEquals(2, query.get().arguments.size());
        assertEquals("Complex query example", query.get().description);
    }

    // =========================================================================
    // MUTATION TESTS
    // =========================================================================

    @Test
    @DisplayName("Create simple mutation")
    void testSimpleMutation() {
        FraiseQL.mutation("createUser")
            .returnType("User")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("createUser");

        assertTrue(mutation.isPresent());
        assertEquals("createUser", mutation.get().name);
        assertEquals("User", mutation.get().returnType);
    }

    @Test
    @DisplayName("Create mutation with arguments")
    void testMutationWithArguments() {
        FraiseQL.mutation("createUser")
            .returnType("User")
            .arg("name", "String")
            .arg("email", "String")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("createUser");

        assertTrue(mutation.isPresent());
        assertEquals(2, mutation.get().arguments.size());
        assertTrue(mutation.get().arguments.containsKey("name"));
        assertTrue(mutation.get().arguments.containsKey("email"));
    }

    @Test
    @DisplayName("Create mutation with class return type")
    void testMutationWithClassReturnType() {
        FraiseQL.registerType(User.class);

        FraiseQL.mutation("updateUser")
            .returnType(User.class)
            .arg("id", "Int")
            .arg("name", "String")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("updateUser");

        assertTrue(mutation.isPresent());
        assertEquals("User", mutation.get().returnType);
    }

    @Test
    @DisplayName("Create mutation with description")
    void testMutationWithDescription() {
        FraiseQL.mutation("deleteUser")
            .returnType("Boolean")
            .arg("id", "Int")
            .description("Delete a user by ID")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("deleteUser");

        assertTrue(mutation.isPresent());
        assertEquals("Delete a user by ID", mutation.get().description);
    }

    @Test
    @DisplayName("Mutation returns array")
    void testMutationReturnsArray() {
        FraiseQL.mutation("batchCreateUsers")
            .returnType("User")
            .returnsArray(true)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("batchCreateUsers");

        assertTrue(mutation.isPresent());
        assertEquals("[User]", mutation.get().returnType);
    }

    // =========================================================================
    // SUBSCRIPTION TESTS
    // =========================================================================

    @Test
    @DisplayName("Create simple subscription")
    void testSimpleSubscription() {
        FraiseQL.subscription("userCreated")
            .entityType("User")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var subscription = registry.getSubscription("userCreated");

        assertTrue(subscription.isPresent());
        assertEquals("userCreated", subscription.get().name);
        assertEquals("User", subscription.get().entityType);
    }

    @Test
    @DisplayName("Create subscription with operation filter")
    void testSubscriptionWithOperation() {
        FraiseQL.subscription("userUpdated")
            .entityType("User")
            .operation("UPDATE")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var subscription = registry.getSubscription("userUpdated");

        assertTrue(subscription.isPresent());
        assertEquals("UPDATE", subscription.get().operation);
    }

    @Test
    @DisplayName("Create subscription with topic")
    void testSubscriptionWithTopic() {
        FraiseQL.subscription("orderEvents")
            .entityType("Order")
            .topic("order_events")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var subscription = registry.getSubscription("orderEvents");

        assertTrue(subscription.isPresent());
        assertEquals("order_events", subscription.get().topic);
    }

    @Test
    @DisplayName("Create subscription with arguments")
    void testSubscriptionWithArguments() {
        FraiseQL.subscription("userUpdatesForId")
            .entityType("User")
            .arg("userId", "Int")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var subscription = registry.getSubscription("userUpdatesForId");

        assertTrue(subscription.isPresent());
        assertEquals(1, subscription.get().arguments.size());
        assertTrue(subscription.get().arguments.containsKey("userId"));
    }

    @Test
    @DisplayName("Create subscription with description")
    void testSubscriptionWithDescription() {
        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .description("Subscribe to new orders")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var subscription = registry.getSubscription("orderCreated");

        assertTrue(subscription.isPresent());
        assertEquals("Subscribe to new orders", subscription.get().description);
    }

    // =========================================================================
    // MIXED OPERATIONS TESTS
    // =========================================================================

    @Test
    @DisplayName("Register all operation types in single schema")
    void testAllOperationTypesInSchema() {
        FraiseQL.registerType(User.class);

        FraiseQL.query("users")
            .returnType(User.class)
            .returnsArray(true)
            .register();

        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .register();

        FraiseQL.subscription("userCreated")
            .entityType(User.class)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();

        assertEquals(1, registry.getAllQueries().size());
        assertEquals(1, registry.getAllMutations().size());
        assertEquals(1, registry.getAllSubscriptions().size());
        assertTrue(registry.getQuery("users").isPresent());
        assertTrue(registry.getMutation("createUser").isPresent());
        assertTrue(registry.getSubscription("userCreated").isPresent());
    }

    @Test
    @DisplayName("Multiple queries, mutations, and subscriptions")
    void testMultipleOperations() {
        FraiseQL.query("users").returnType("User").register();
        FraiseQL.query("posts").returnType("Post").register();
        FraiseQL.query("comments").returnType("Comment").register();

        FraiseQL.mutation("createUser").returnType("User").register();
        FraiseQL.mutation("createPost").returnType("Post").register();

        FraiseQL.subscription("userCreated").entityType("User").register();
        FraiseQL.subscription("postCreated").entityType("Post").register();

        SchemaRegistry registry = SchemaRegistry.getInstance();

        assertEquals(3, registry.getAllQueries().size());
        assertEquals(2, registry.getAllMutations().size());
        assertEquals(2, registry.getAllSubscriptions().size());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType(description = "A user account")
    public static class User {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;

        @GraphQLField
        public String email;
    }

    @GraphQLType(description = "A blog post")
    public static class Post {
        @GraphQLField
        public int id;

        @GraphQLField
        public String title;
    }

    @GraphQLType(description = "A comment")
    public static class Comment {
        @GraphQLField
        public int id;

        @GraphQLField
        public String text;
    }

    @GraphQLType(description = "An order")
    public static class Order {
        @GraphQLField
        public int id;

        @GraphQLField
        public String status;
    }
}
