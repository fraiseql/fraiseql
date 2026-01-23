package com.fraiseql.core;

import java.util.*;

/**
 * Fluent builder for creating FraiseQL observers programmatically.
 *
 * <p>Observers listen to database change events (INSERT/UPDATE/DELETE) and execute
 * actions (webhooks, Slack, email) when conditions are met.</p>
 *
 * <p>Example usage:</p>
 * <pre>
 * {@code
 * new ObserverBuilder("onHighValueOrder")
 *     .entity("Order")
 *     .event("INSERT")
 *     .condition("total > 1000")
 *     .addAction(Webhook.create("https://api.example.com/orders"))
 *     .addAction(SlackAction.create("#sales", "New order: {id}"))
 *     .retry(RetryConfig.exponential(5, 100, 60000))
 *     .register();
 * }
 * </pre>
 */
public class ObserverBuilder {
    private final String name;
    private String entity;
    private String event;
    private String condition;
    private final List<Map<String, Object>> actions = new ArrayList<>();
    private RetryConfig retry = RetryConfig.defaults();

    public ObserverBuilder(String name) {
        this.name = name;
    }

    /**
     * Set the entity type to observe (e.g., "Order", "User").
     */
    public ObserverBuilder entity(String entity) {
        this.entity = entity;
        return this;
    }

    /**
     * Set the event type: INSERT, UPDATE, or DELETE.
     */
    public ObserverBuilder event(String event) {
        this.event = event.toUpperCase();
        return this;
    }

    /**
     * Set optional condition expression in FraiseQL DSL.
     * Examples: "total > 1000", "status.changed() and status == 'shipped'"
     */
    public ObserverBuilder condition(String condition) {
        this.condition = condition;
        return this;
    }

    /**
     * Add an action to execute when observer triggers.
     */
    public ObserverBuilder addAction(Map<String, Object> action) {
        this.actions.add(action);
        return this;
    }

    /**
     * Set retry configuration for action execution.
     */
    public ObserverBuilder retry(RetryConfig retry) {
        this.retry = retry;
        return this;
    }

    /**
     * Register the observer with the global schema registry.
     */
    public void register() {
        if (name == null || entity == null || event == null) {
            throw new IllegalStateException("Observer must have name, entity, and event");
        }
        if (actions.isEmpty()) {
            throw new IllegalStateException("Observer must have at least one action");
        }

        SchemaRegistry.getInstance().registerObserver(
            name, entity, event, actions, condition, retry
        );
    }
}

/**
 * Retry configuration for observer action execution.
 */
class RetryConfig {
    private final int maxAttempts;
    private final String backoffStrategy;
    private final int initialDelayMs;
    private final int maxDelayMs;

    public RetryConfig(int maxAttempts, String backoffStrategy, int initialDelayMs, int maxDelayMs) {
        this.maxAttempts = maxAttempts;
        this.backoffStrategy = backoffStrategy;
        this.initialDelayMs = initialDelayMs;
        this.maxDelayMs = maxDelayMs;
    }

    public static RetryConfig defaults() {
        return new RetryConfig(3, "exponential", 100, 60000);
    }

    public static RetryConfig exponential(int maxAttempts, int initialDelayMs, int maxDelayMs) {
        return new RetryConfig(maxAttempts, "exponential", initialDelayMs, maxDelayMs);
    }

    public static RetryConfig linear(int maxAttempts, int initialDelayMs, int maxDelayMs) {
        return new RetryConfig(maxAttempts, "linear", initialDelayMs, maxDelayMs);
    }

    public static RetryConfig fixed(int maxAttempts, int delayMs) {
        return new RetryConfig(maxAttempts, "fixed", delayMs, delayMs);
    }

    public int getMaxAttempts() {
        return maxAttempts;
    }

    public String getBackoffStrategy() {
        return backoffStrategy;
    }

    public int getInitialDelayMs() {
        return initialDelayMs;
    }

    public int getMaxDelayMs() {
        return maxDelayMs;
    }

    public Map<String, Object> toMap() {
        Map<String, Object> map = new LinkedHashMap<>();
        map.put("max_attempts", maxAttempts);
        map.put("backoff_strategy", backoffStrategy);
        map.put("initial_delay_ms", initialDelayMs);
        map.put("max_delay_ms", maxDelayMs);
        return map;
    }
}

/**
 * Builder for webhook actions.
 */
class Webhook {
    public static Map<String, Object> create(String url) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "webhook");
        action.put("url", url);
        Map<String, String> headers = new LinkedHashMap<>();
        headers.put("Content-Type", "application/json");
        action.put("headers", headers);
        return action;
    }

    public static Map<String, Object> withEnv(String urlEnv) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "webhook");
        action.put("url_env", urlEnv);
        Map<String, String> headers = new LinkedHashMap<>();
        headers.put("Content-Type", "application/json");
        action.put("headers", headers);
        return action;
    }

    public static Map<String, Object> withOptions(String url, Map<String, Object> options) {
        Map<String, Object> action = create(url);
        action.putAll(options);
        return action;
    }
}

/**
 * Builder for Slack actions.
 */
class SlackAction {
    public static Map<String, Object> create(String channel, String message) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "slack");
        action.put("channel", channel);
        action.put("message", message);
        action.put("webhook_url_env", "SLACK_WEBHOOK_URL");
        return action;
    }

    public static Map<String, Object> withWebhookUrl(String channel, String message, String webhookUrl) {
        Map<String, Object> action = create(channel, message);
        action.remove("webhook_url_env");
        action.put("webhook_url", webhookUrl);
        return action;
    }

    public static Map<String, Object> withEnv(String channel, String message, String webhookEnv) {
        Map<String, Object> action = create(channel, message);
        action.put("webhook_url_env", webhookEnv);
        return action;
    }
}

/**
 * Builder for email actions.
 */
class EmailAction {
    public static Map<String, Object> create(String to, String subject, String body) {
        Map<String, Object> action = new LinkedHashMap<>();
        action.put("type", "email");
        action.put("to", to);
        action.put("subject", subject);
        action.put("body", body);
        return action;
    }

    public static Map<String, Object> withFrom(String to, String subject, String body, String fromEmail) {
        Map<String, Object> action = create(to, subject, body);
        action.put("from", fromEmail);
        return action;
    }
}
