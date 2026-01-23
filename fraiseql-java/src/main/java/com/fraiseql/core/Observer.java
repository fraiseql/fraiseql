package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Annotation to define a FraiseQL observer that listens to database change events.
 * Observers react to INSERT/UPDATE/DELETE operations on entities with configurable actions.
 *
 * <p>Example usage:</p>
 * <pre>
 * {@code
 * @Observer(
 *     name = "onHighValueOrder",
 *     entity = "Order",
 *     event = "INSERT",
 *     condition = "total > 1000"
 * )
 * public class HighValueOrderObserver {
 *     // Actions are registered separately using ObserverBuilder
 * }
 * }
 * </pre>
 *
 * <p>Note: This annotation marks the class. Actions must be registered programmatically
 * using {@link ObserverBuilder} to define webhooks, Slack notifications, or emails.</p>
 *
 * @see ObserverBuilder
 * @see WebhookAction
 * @see SlackAction
 * @see EmailAction
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface Observer {
    /**
     * The observer name (unique identifier).
     */
    String name();

    /**
     * The entity type to observe (e.g., "Order", "User").
     */
    String entity();

    /**
     * The database event type: INSERT, UPDATE, or DELETE.
     */
    String event();

    /**
     * Optional condition expression in FraiseQL DSL.
     * Examples:
     * - "total > 1000"
     * - "status.changed() and status == 'shipped'"
     * - "amount >= 500 and currency == 'USD'"
     */
    String condition() default "";

    /**
     * Optional description of the observer's purpose.
     */
    String description() default "";
}
