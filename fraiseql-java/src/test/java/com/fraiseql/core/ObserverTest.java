package com.fraiseql.core;

import org.junit.jupiter.api.*;
import static org.junit.jupiter.api.Assertions.*;

import java.util.*;

/**
 * Tests for FraiseQL observer authoring API.
 */
@TestMethodOrder(MethodOrderer.OrderAnnotation.class)
public class ObserverTest {
    private final SchemaRegistry registry = SchemaRegistry.getInstance();

    @BeforeEach
    void setup() {
        registry.clear();
    }

    @Test
    @Order(1)
    @DisplayName("Observer builder registers with schema registry")
    void testObserverBuilder() {
        new ObserverBuilder("onOrderCreated")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com/orders"))
            .register();

        var observers = registry.getAllObservers();
        assertEquals(1, observers.size());

        var observer = observers.get("onOrderCreated");
        assertNotNull(observer);
        assertEquals("onOrderCreated", observer.name);
        assertEquals("Order", observer.entity);
        assertEquals("INSERT", observer.event);
        assertEquals(1, observer.actions.size());
    }

    @Test
    @Order(2)
    @DisplayName("Observer with condition")
    void testObserverWithCondition() {
        new ObserverBuilder("onHighValueOrder")
            .entity("Order")
            .event("INSERT")
            .condition("total > 1000")
            .addAction(Webhook.create("https://example.com/high-value"))
            .register();

        var observer = registry.getAllObservers().get("onHighValueOrder");
        assertNotNull(observer);
        assertEquals("total > 1000", observer.condition);
    }

    @Test
    @Order(3)
    @DisplayName("Observer with custom retry")
    void testObserverWithCustomRetry() {
        var customRetry = RetryConfig.exponential(5, 200, 30000);

        new ObserverBuilder("onOrder")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com"))
            .retry(customRetry)
            .register();

        var observer = registry.getAllObservers().get("onOrder");
        assertNotNull(observer);
        assertEquals(5, observer.retry.getMaxAttempts());
        assertEquals("exponential", observer.retry.getBackoffStrategy());
        assertEquals(200, observer.retry.getInitialDelayMs());
        assertEquals(30000, observer.retry.getMaxDelayMs());
    }

    @Test
    @Order(4)
    @DisplayName("Observer with default retry")
    void testObserverWithDefaultRetry() {
        new ObserverBuilder("onOrder")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com"))
            .register();

        var observer = registry.getAllObservers().get("onOrder");
        assertNotNull(observer);
        assertEquals(3, observer.retry.getMaxAttempts());
        assertEquals("exponential", observer.retry.getBackoffStrategy());
        assertEquals(100, observer.retry.getInitialDelayMs());
        assertEquals(60000, observer.retry.getMaxDelayMs());
    }

    @Test
    @Order(5)
    @DisplayName("Observer with multiple actions")
    void testObserverWithMultipleActions() {
        new ObserverBuilder("onOrder")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com/orders"))
            .addAction(SlackAction.create("#orders", "New order {id}"))
            .addAction(EmailAction.create("admin@example.com", "Order created", "Order {id} created"))
            .register();

        var observer = registry.getAllObservers().get("onOrder");
        assertNotNull(observer);
        assertEquals(3, observer.actions.size());
        assertEquals("webhook", observer.actions.get(0).get("type"));
        assertEquals("slack", observer.actions.get(1).get("type"));
        assertEquals("email", observer.actions.get(2).get("type"));
    }

    @Test
    @Order(6)
    @DisplayName("Webhook action")
    void testWebhookAction() {
        var action = Webhook.create("https://example.com/orders");

        assertEquals("webhook", action.get("type"));
        assertEquals("https://example.com/orders", action.get("url"));
        assertNotNull(action.get("headers"));
    }

    @Test
    @Order(7)
    @DisplayName("Webhook with environment variable")
    void testWebhookWithEnv() {
        var action = Webhook.withEnv("ORDER_WEBHOOK_URL");

        assertEquals("webhook", action.get("type"));
        assertEquals("ORDER_WEBHOOK_URL", action.get("url_env"));
        assertFalse(action.containsKey("url"));
    }

    @Test
    @Order(8)
    @DisplayName("Slack action")
    void testSlackAction() {
        var action = SlackAction.create("#orders", "New order {id}");

        assertEquals("slack", action.get("type"));
        assertEquals("#orders", action.get("channel"));
        assertEquals("New order {id}", action.get("message"));
        assertEquals("SLACK_WEBHOOK_URL", action.get("webhook_url_env"));
    }

    @Test
    @Order(9)
    @DisplayName("Slack with custom webhook")
    void testSlackWithCustomWebhook() {
        var action = SlackAction.withWebhookUrl("#orders", "New order",
            "https://hooks.slack.com/services/XXX");

        assertEquals("https://hooks.slack.com/services/XXX", action.get("webhook_url"));
        assertFalse(action.containsKey("webhook_url_env"));
    }

    @Test
    @Order(10)
    @DisplayName("Email action")
    void testEmailAction() {
        var action = EmailAction.create("admin@example.com",
            "Order {id} created",
            "Order {id} for ${total} was created");

        assertEquals("email", action.get("type"));
        assertEquals("admin@example.com", action.get("to"));
        assertEquals("Order {id} created", action.get("subject"));
        assertEquals("Order {id} for ${total} was created", action.get("body"));
    }

    @Test
    @Order(11)
    @DisplayName("Email with from address")
    void testEmailWithFrom() {
        var action = EmailAction.withFrom("customer@example.com",
            "Order shipped",
            "Your order is on its way!",
            "noreply@example.com");

        assertEquals("noreply@example.com", action.get("from"));
    }

    @Test
    @Order(12)
    @DisplayName("Schema export includes observers")
    void testSchemaExportWithObservers() {
        new ObserverBuilder("onOrder1")
            .entity("Order")
            .event("INSERT")
            .addAction(Webhook.create("https://example.com"))
            .register();

        new ObserverBuilder("onOrder2")
            .entity("Order")
            .event("UPDATE")
            .addAction(SlackAction.create("#orders", "Updated"))
            .register();

        var observers = registry.getAllObservers();
        assertEquals(2, observers.size());
        assertTrue(observers.containsKey("onOrder1"));
        assertTrue(observers.containsKey("onOrder2"));
    }

    @Test
    @Order(13)
    @DisplayName("Empty observers collection")
    void testEmptyObservers() {
        var observers = registry.getAllObservers();
        assertEquals(0, observers.size());
    }
}
