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
            .addAction(SlackAction.create("#orders", "New order {{ id }}"))
            .addAction(EmailAction.create("admin@example.com", "Order created", "Order {{ id }} created"))
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
        var action = SlackAction.create("#orders", "New order {{ id }}");

        assertEquals(Map.of(
            "type", "slack",
            "channel", "#orders",
            "message_template", "New order {{ id }}",
            "webhook_url_env", "SLACK_WEBHOOK_URL"), action);
    }

    @Test
    @Order(9)
    @DisplayName("Slack with custom webhook")
    void testSlackWithCustomWebhook() {
        var action = SlackAction.withWebhookUrl("#orders", "New order",
            "https://hooks.slack.com/services/XXX");

        assertEquals("https://hooks.slack.com/services/XXX", action.get("webhook_url"));
        assertEquals("New order", action.get("message_template"));
        assertFalse(action.containsKey("webhook_url_env"));
        assertFalse(action.containsKey("message"));
    }

    @Test
    @Order(10)
    @DisplayName("Email action")
    void testEmailAction() {
        var action = EmailAction.create("admin@example.com",
            "Order created",
            "Order {{ id }} for ${{ total }} was created");

        assertEquals(Map.of(
            "type", "email",
            "to", "admin@example.com",
            "subject", "Order created",
            "body_template", "Order {{ id }} for ${{ total }} was created"), action);
    }

    @Test
    @Order(11)
    @DisplayName("Webhook body template and signing variable")
    void testWebhookBodyTemplateAndSigning() {
        var action = Webhook.signedWith(
            Webhook.bodyTemplate(Webhook.withEnv("ORDER_WEBHOOK_URL"), "{\"id\": \"{{ id }}\"}"),
            "ORDER_WEBHOOK_SECRET");

        assertEquals(
            Set.of("type", "url_env", "headers", "body_template", "signing_secret_env"),
            action.keySet());
        assertEquals("{\"id\": \"{{ id }}\"}", action.get("body_template"));
        assertEquals("ORDER_WEBHOOK_SECRET", action.get("signing_secret_env"));
    }

    @Test
    @Order(11)
    @DisplayName("Webhook header value from an environment variable")
    void testWebhookHeaderFromEnv() {
        var action = Webhook.headerFromEnv(
            Webhook.headerFromEnv(Webhook.create("https://example.com/hook"),
                "Authorization", "ORDER_HOOK_TOKEN"),
            "X-Api-Key", "ORDER_HOOK_KEY");

        assertEquals(
            Map.of("Authorization", "ORDER_HOOK_TOKEN", "X-Api-Key", "ORDER_HOOK_KEY"),
            action.get("headers_env"));
        assertEquals(Map.of(), action.get("headers"), "no value is written under headers");
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

        var exported = SchemaFormatter.formatSchema(registry).get("observers");
        assertNotNull(exported, "the exported schema carries the observers");
        assertEquals(2, exported.size());

        var first = exported.get(0);
        assertEquals("onOrder1", first.get("name").asText());
        assertEquals("Order", first.get("entity").asText());
        assertEquals("INSERT", first.get("event").asText());
        assertFalse(first.has("condition"));
        assertEquals("https://example.com", first.get("actions").get(0).get("url").asText());
        var retry = first.get("retry");
        assertEquals(
            Set.of("max_attempts", "backoff_strategy", "initial_delay_ms", "max_delay_ms"),
            Set.copyOf(iterable(retry.fieldNames())));
        assertEquals(3, retry.get("max_attempts").asInt());
        assertEquals("exponential", retry.get("backoff_strategy").asText());
        assertEquals(100, retry.get("initial_delay_ms").asInt());
        assertEquals(60000, retry.get("max_delay_ms").asInt());

        var slack = exported.get(1).get("actions").get(0);
        assertEquals("Updated", slack.get("message_template").asText());
        assertEquals("SLACK_WEBHOOK_URL", slack.get("webhook_url_env").asText());
    }

    private static List<String> iterable(Iterator<String> names) {
        List<String> out = new ArrayList<>();
        names.forEachRemaining(out::add);
        return out;
    }

    @Test
    @Order(14)
    @DisplayName("A schema with no observers exports no observers key")
    void testNoObserversKeyWhenNoneRegistered() {
        assertFalse(SchemaFormatter.formatSchema(registry).has("observers"));
    }

    @Test
    @Order(15)
    @DisplayName("Linear retry")
    void testLinearRetry() {
        var retry = RetryConfig.linear(4, 250, 10_000);

        assertEquals("linear", retry.getBackoffStrategy());
        assertEquals(4, retry.getMaxAttempts());
        assertEquals(250, retry.getInitialDelayMs());
        assertEquals(10_000, retry.getMaxDelayMs());
    }

    @Test
    @Order(13)
    @DisplayName("Empty observers collection")
    void testEmptyObservers() {
        var observers = registry.getAllObservers();
        assertEquals(0, observers.size());
    }
}
