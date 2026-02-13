package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL subscription support.
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 */
public class SubscriptionTest {

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
    }

    /**
     * Test registering a simple subscription
     */
    @Test
    public void testSimpleSubscription() {
        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .description("Subscribe to new orders")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.SubscriptionInfo> subInfo = registry.getSubscription("orderCreated");

        assertTrue(subInfo.isPresent());
        assertEquals("orderCreated", subInfo.get().name);
        assertEquals("Order", subInfo.get().entityType);
        assertEquals("Subscribe to new orders", subInfo.get().description);
    }

    /**
     * Test subscription with topic
     */
    @Test
    public void testSubscriptionWithTopic() {
        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .topic("order_events")
            .description("Subscribe to new orders")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.SubscriptionInfo> subInfo = registry.getSubscription("orderCreated");

        assertTrue(subInfo.isPresent());
        assertEquals("order_events", subInfo.get().topic);
    }

    /**
     * Test subscription with operation filter
     */
    @Test
    public void testSubscriptionWithOperation() {
        FraiseQL.subscription("userUpdated")
            .entityType("User")
            .operation("UPDATE")
            .description("Subscribe to user updates")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.SubscriptionInfo> subInfo = registry.getSubscription("userUpdated");

        assertTrue(subInfo.isPresent());
        assertEquals("UPDATE", subInfo.get().operation);
    }

    /**
     * Test subscription with arguments
     */
    @Test
    public void testSubscriptionWithArguments() {
        FraiseQL.subscription("orderStatusChanged")
            .entityType("Order")
            .arg("userId", "String")
            .arg("status", "String")
            .description("Subscribe to order status changes")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.SubscriptionInfo> subInfo = registry.getSubscription("orderStatusChanged");

        assertTrue(subInfo.isPresent());
        assertEquals(2, subInfo.get().arguments.size());
        assertTrue(subInfo.get().arguments.containsKey("userId"));
        assertTrue(subInfo.get().arguments.containsKey("status"));
    }

    /**
     * Test subscription with class entity type
     */
    @Test
    public void testSubscriptionWithClassEntityType() {
        FraiseQL.registerType(Order.class);

        FraiseQL.subscription("orderCreated")
            .entityType(Order.class)
            .description("Subscribe to new orders")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.SubscriptionInfo> subInfo = registry.getSubscription("orderCreated");

        assertTrue(subInfo.isPresent());
        assertEquals("Order", subInfo.get().entityType);
    }

    /**
     * Test registering multiple subscriptions
     */
    @Test
    public void testMultipleSubscriptions() {
        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .register();

        FraiseQL.subscription("orderUpdated")
            .entityType("Order")
            .operation("UPDATE")
            .register();

        FraiseQL.subscription("userCreated")
            .entityType("User")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertEquals(3, registry.getAllSubscriptions().size());
        assertTrue(registry.getSubscription("orderCreated").isPresent());
        assertTrue(registry.getSubscription("orderUpdated").isPresent());
        assertTrue(registry.getSubscription("userCreated").isPresent());
    }

    /**
     * Test that clear() removes subscriptions
     */
    @Test
    public void testClearRemovesSubscriptions() {
        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        assertEquals(1, registry.getAllSubscriptions().size());

        FraiseQL.clear();

        assertEquals(0, registry.getAllSubscriptions().size());
    }

    /**
     * Test complete subscription with all options
     */
    @Test
    public void testCompleteSubscription() {
        FraiseQL.subscription("orderCreated")
            .entityType("Order")
            .arg("storeId", "Int")
            .topic("order_events")
            .operation("CREATE")
            .description("Subscribe to new orders")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.SubscriptionInfo> subInfo = registry.getSubscription("orderCreated");

        assertTrue(subInfo.isPresent());
        assertEquals("orderCreated", subInfo.get().name);
        assertEquals("Order", subInfo.get().entityType);
        assertEquals("order_events", subInfo.get().topic);
        assertEquals("CREATE", subInfo.get().operation);
        assertEquals("Subscribe to new orders", subInfo.get().description);
        assertEquals(1, subInfo.get().arguments.size());
    }

    // Test fixture class
    @GraphQLType(description = "An order in the system")
    public static class Order {
        @GraphQLField
        public int id;

        @GraphQLField
        public String status;

        @GraphQLField
        public double total;
    }
}
