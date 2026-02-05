package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Java ↔ TypeScript/Python Feature Parity Tests
 *
 * This test suite validates that Java can express equivalent features
 * as TypeScript and Python implementations.
 */
@DisplayName("Java Feature Parity")
public class ParityTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear();
    }

    // =========================================================================
    // TYPE SYSTEM PARITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Parity: Register type with basic scalar fields")
    void testParityTypeWithBasicScalars() {
        // TypeScript equivalent:
        // registerTypeFields("User", [
        //   { name: "id", type: "ID", nullable: false },
        //   { name: "email", type: "Email", nullable: false },
        //   { name: "age", type: "Int", nullable: false },
        // ])

        FraiseQL.registerType(ParityUser.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("ParityUser");

        assertTrue(typeInfo.isPresent());
        assertEquals(3, typeInfo.get().fields.size());
        assertTrue(typeInfo.get().fields.containsKey("id"));
        assertTrue(typeInfo.get().fields.containsKey("email"));
        assertTrue(typeInfo.get().fields.containsKey("age"));
    }

    @Test
    @DisplayName("Parity: Register query with parameters")
    void testParityQueryWithParameters() {
        // TypeScript equivalent:
        // registerQuery("users", "User", true, false, [
        //   { name: "limit", type: "Int", nullable: false, default: 10 },
        //   { name: "offset", type: "Int", nullable: false, default: 0 },
        // ])

        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .arg("limit", "Int")
            .arg("offset", "Int")
            .description("Get list of users")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("users");

        assertTrue(query.isPresent());
        assertEquals("[User]", query.get().returnType);
        assertEquals(2, query.get().arguments.size());
    }

    @Test
    @DisplayName("Parity: Register mutation with operation")
    void testParityMutationWithOperation() {
        // TypeScript equivalent:
        // registerMutation("createUser", "User", false, false, [
        //   { name: "name", type: "String", nullable: false },
        //   { name: "email", type: "String", nullable: false },
        // ], "Create a new user", { operation: "CREATE" })

        FraiseQL.mutation("createUser")
            .returnType("User")
            .arg("name", "String")
            .arg("email", "String")
            .description("Create a new user")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("createUser");

        assertTrue(mutation.isPresent());
        assertEquals("User", mutation.get().returnType);
        assertEquals(2, mutation.get().arguments.size());
    }

    @Test
    @DisplayName("Parity: Register subscription with event filtering")
    void testParitySubscriptionWithEventFiltering() {
        // TypeScript equivalent:
        // registerSubscription("orderCreated", "Order", false, [
        //   { name: "userId", type: "String", nullable: true }
        // ], "Subscribe to new orders", { operation: "CREATE" })

        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .arg("userId", "String")
            .description("Subscribe to new orders")
            .operation("CREATE")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var subscription = registry.getSubscription("orderCreated");

        assertTrue(subscription.isPresent());
        assertEquals("Order", subscription.get().entityType);
        assertEquals("CREATE", subscription.get().operation);
        assertEquals(1, subscription.get().arguments.size());
    }

    // =========================================================================
    // OPERATIONS PARITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Parity: Query with array return type")
    void testParityQueryArrayReturn() {
        // TypeScript: registerQuery("users", "User", true, ...)
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var query = registry.getQuery("users");

        assertTrue(query.isPresent());
        assertTrue(query.get().returnType.startsWith("["));
        assertTrue(query.get().returnType.endsWith("]"));
    }

    @Test
    @DisplayName("Parity: Mutation returns single type")
    void testParityMutationSingleReturn() {
        // TypeScript: registerMutation("createUser", "User", false, ...)
        FraiseQL.mutation("createUser")
            .returnType("User")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var mutation = registry.getMutation("createUser");

        assertTrue(mutation.isPresent());
        assertEquals("User", mutation.get().returnType);
        assertFalse(mutation.get().returnType.startsWith("["));
    }

    @Test
    @DisplayName("Parity: Multiple subscriptions for same entity")
    void testParityMultipleSubscriptionsPerEntity() {
        // TypeScript allows multiple subscriptions on same entity with different filters

        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .operation("CREATE")
            .register();

        FraiseQL.subscription("orderUpdated")
            .entityType("Order")
            .operation("UPDATE")
            .register();

        FraiseQL.subscription("orderDeleted")
            .entityType("Order")
            .operation("DELETE")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();

        assertEquals(3, registry.getAllSubscriptions().size());
        assertTrue(registry.getSubscription("orderCreated").isPresent());
        assertTrue(registry.getSubscription("orderUpdated").isPresent());
        assertTrue(registry.getSubscription("orderDeleted").isPresent());
    }

    // =========================================================================
    // FIELD METADATA PARITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Parity: Field with description")
    void testParityFieldDescription() {
        // TypeScript: { name: "salary", type: "Decimal", description: "User salary" }

        FraiseQL.registerType(UserWithFieldMetadata.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("UserWithFieldMetadata");

        assertTrue(typeInfo.isPresent());
        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("email"));
    }

    @Test
    @DisplayName("Parity: Multiple fields with descriptions")
    void testParityMultipleFieldDescriptions() {
        FraiseQL.registerType(DocumentedType.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var typeInfo = registry.getType("DocumentedType");

        assertTrue(typeInfo.isPresent());
        assertEquals(3, typeInfo.get().fields.size());
    }

    // =========================================================================
    // OBSERVER PARITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Parity: Observer with webhook action")
    void testParityObserverWebhook() {
        // TypeScript equivalent:
        // registerObserver("notifyOnOrderCreated", "Order", "INSERT", [
        //   { type: "webhook", url: "https://example.com/webhooks/order" }
        // ])

        new ObserverBuilder("notifyOnOrderCreated")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com/webhooks/order"))
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var observer = registry.getAllObservers().get("notifyOnOrderCreated");

        assertNotNull(observer);
        assertEquals("Order", observer.entity);
        assertEquals("INSERT", observer.event);
        assertEquals(1, observer.actions.size());
    }

    @Test
    @DisplayName("Parity: Observer with retry configuration")
    void testParityObserverRetryConfig() {
        // TypeScript equivalent with RetryConfig:
        // registerObserver(..., {
        //   max_attempts: 5,
        //   backoff_strategy: "exponential",
        //   initial_delay_ms: 200,
        //   max_delay_ms: 30000
        // })

        var retryConfig = RetryConfig.exponential(5, 200, 30000);

        new ObserverBuilder("criticalOrder")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com/critical"))
            .retry(retryConfig)
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        var observer = registry.getAllObservers().get("criticalOrder");

        assertNotNull(observer);
        assertEquals(5, observer.retry.getMaxAttempts());
        assertEquals("exponential", observer.retry.getBackoffStrategy());
        assertEquals(200, observer.retry.getInitialDelayMs());
        assertEquals(30000, observer.retry.getMaxDelayMs());
    }

    // =========================================================================
    // COMPLETE SCHEMA PARITY TEST
    // =========================================================================

    @Test
    @DisplayName("Parity: Complete schema with all feature types")
    void testParityCompleteSchema() {
        // Register types
        FraiseQL.registerTypes(ParityUser.class, ParityOrder.class);

        // Register queries
        FraiseQL.query("users")
            .returnType("ParityUser")
            .returnsArray(true)
            .arg("limit", "Int")
            .register();

        FraiseQL.query("orders")
            .returnType("ParityOrder")
            .returnsArray(true)
            .register();

        // Register mutations
        FraiseQL.mutation("createUser")
            .returnType("ParityUser")
            .arg("name", "String")
            .register();

        FraiseQL.mutation("createOrder")
            .returnType("ParityOrder")
            .arg("userId", "Int")
            .register();

        // Register subscriptions
        FraiseQL.subscription("userCreated")
            .entityType("ParityUser")
            .operation("CREATE")
            .register();

        FraiseQL.subscription("orderCreated")
            .entityType("ParityOrder")
            .operation("CREATE")
            .register();

        // Register observer
        new ObserverBuilder("onOrder")
            .entity("ParityOrder")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com/orders"))
            .register();

        // Verify completeness
        SchemaRegistry registry = SchemaRegistry.getInstance();

        assertEquals(2, registry.getAllTypes().size());
        assertEquals(2, registry.getAllQueries().size());
        assertEquals(2, registry.getAllMutations().size());
        assertEquals(2, registry.getAllSubscriptions().size());
        assertEquals(1, registry.getAllObservers().size());
    }

    // =========================================================================
    // TYPE CONVERSION PARITY TESTS
    // =========================================================================

    @Test
    @DisplayName("Parity: Java int → GraphQL Int")
    void testParityJavaIntToGraphQLInt() {
        // Both TypeScript and Java map int to Int
        assertEquals("Int", TypeConverter.javaToGraphQL(int.class));
        assertEquals("Int", TypeConverter.javaToGraphQL(Integer.class));
    }

    @Test
    @DisplayName("Parity: Java String → GraphQL String")
    void testParityJavaStringToGraphQLString() {
        // Both TypeScript and Java map String to String
        assertEquals("String", TypeConverter.javaToGraphQL(String.class));
    }

    @Test
    @DisplayName("Parity: Java boolean → GraphQL Boolean")
    void testParityJavaBooleanToGraphQLBoolean() {
        // Both TypeScript and Java map boolean to Boolean
        assertEquals("Boolean", TypeConverter.javaToGraphQL(boolean.class));
        assertEquals("Boolean", TypeConverter.javaToGraphQL(Boolean.class));
    }

    @Test
    @DisplayName("Parity: Java float → GraphQL Float")
    void testParityJavaFloatToGraphQLFloat() {
        // Both TypeScript and Java map float/double to Float
        assertEquals("Float", TypeConverter.javaToGraphQL(float.class));
        assertEquals("Float", TypeConverter.javaToGraphQL(Float.class));
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLType
    public static class ParityUser {
        @GraphQLField
        public int id;

        @GraphQLField
        public String email;

        @GraphQLField
        public int age;
    }

    @GraphQLType
    public static class ParityOrder {
        @GraphQLField
        public int id;

        @GraphQLField
        public int userId;

        @GraphQLField
        public String status;
    }

    @GraphQLType
    public static class UserWithFieldMetadata {
        @GraphQLField(description = "User ID")
        public int id;

        @GraphQLField(description = "Email address")
        public String email;

        @GraphQLField(description = "User's full name")
        public String name;
    }

    @GraphQLType
    public static class DocumentedType {
        @GraphQLField(description = "Primary identifier")
        public int id;

        @GraphQLField(description = "Entity name")
        public String name;

        @GraphQLField(description = "Creation timestamp")
        public java.time.LocalDateTime createdAt;
    }
}
