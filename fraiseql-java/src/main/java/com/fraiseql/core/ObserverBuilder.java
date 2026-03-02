package com.fraiseql.core;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Fluent builder for FraiseQL observer definitions.
 * Observers react to database events and dispatch configured actions.
 *
 * <p>Example:
 * <pre>
 * new ObserverBuilder("onOrderCreated")
 *     .entity("Order")
 *     .event("INSERT")
 *     .addAction(Webhook.create("https://example.com/hook"))
 *     .register();
 * </pre>
 */
public final class ObserverBuilder {

    private final String name;
    private String entity;
    private String event;
    private String condition;
    private final List<Map<String, Object>> actions = new ArrayList<>();
    private RetryConfig retry = RetryConfig.defaults();

    private static final SchemaRegistry registry = SchemaRegistry.getInstance();

    /**
     * @param name unique observer name
     */
    public ObserverBuilder(String name) {
        this.name = name;
    }

    /**
     * Set the entity type that triggers this observer.
     *
     * @param entity GraphQL type name
     * @return this builder
     */
    public ObserverBuilder entity(String entity) {
        this.entity = entity;
        return this;
    }

    /**
     * Set the database event that triggers this observer.
     *
     * @param event INSERT, UPDATE, or DELETE
     * @return this builder
     */
    public ObserverBuilder event(String event) {
        this.event = event;
        return this;
    }

    /**
     * Optional CEL-like condition; the observer fires only when true.
     *
     * @param condition filter expression
     * @return this builder
     */
    public ObserverBuilder condition(String condition) {
        this.condition = condition;
        return this;
    }

    /**
     * Add an action to execute when the observer fires.
     * Use {@link Webhook}, {@link SlackAction}, or {@link EmailAction} factories.
     *
     * @param action action map produced by an action factory
     * @return this builder
     */
    public ObserverBuilder addAction(Map<String, Object> action) {
        actions.add(action);
        return this;
    }

    /**
     * Override the default retry configuration.
     *
     * @param retry retry configuration
     * @return this builder
     */
    public ObserverBuilder retry(RetryConfig retry) {
        this.retry = retry;
        return this;
    }

    /**
     * Register this observer in the schema registry.
     *
     * @throws IllegalStateException if entity or event is missing
     */
    public void register() {
        if (entity == null || entity.isEmpty()) {
            throw new IllegalStateException("Observer '" + name + "': entity must be set");
        }
        if (event == null || event.isEmpty()) {
            throw new IllegalStateException("Observer '" + name + "': event must be set");
        }
        registry.registerObserver(name, entity, event, condition, retry, new ArrayList<>(actions));
    }
}
