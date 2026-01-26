package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field or type with custom authorization rules.
 * Enables fine-grained access control beyond simple scope checking.
 *
 * Supports both field-level and type-level authorization with custom rule expressions.
 *
 * Usage:
 * <pre>
 * @GraphQLType
 * @Authorize(rule = "isOwner($context.userId, $field.ownerId)")
 * public class PrivateNote {
 *     @GraphQLField
 *     public String id;
 *
 *     @GraphQLField
 *     @Authorize(rule = "isOwner($context.userId, $field.ownerId) OR hasRole($context, 'admin')")
 *     public String content;
 * }
 *
 * // Financial data with attribute-based access
 * @GraphQLType
 * @Authorize(rule = "hasAttribute($context, 'clearance_level', 3)")
 * public class FinancialRecord {
 *     @GraphQLField
 *     public float amount;
 * }
 *
 * // Custom business logic
 * @GraphQLField
 * @Authorize(policy = "piiAccess", rule = "canAccessPII($context)")
 * public String socialSecurityNumber;
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.FIELD})
public @interface Authorize {
    /**
     * Custom authorization rule expression.
     * The expression can use context variables like:
     * - $context.userId: Current user ID
     * - $context.roles: User's roles array
     * - $context.attributes: User's attributes map
     * - $field.*: Field properties
     * - $parent.*: Parent object properties
     *
     * Example: "isOwner($context.userId, $field.ownerId)"
     */
    String rule() default "";

    /**
     * Reference to a named authorization policy.
     * Policies can be registered via AuthzPolicy decorator.
     *
     * Example: "piiAccess", "financialData", "adminOnly"
     */
    String policy() default "";

    /**
     * Description of what this authorization rule protects.
     * Should explain the business rationale for the access control.
     *
     * Example: "Ensures users can only access their own notes"
     */
    String description() default "";

    /**
     * Optional error message to return when authorization fails.
     * If not specified, a generic message is used.
     *
     * Example: "You do not have permission to access this field"
     */
    String errorMessage() default "";

    /**
     * Whether to apply this rule hierarchically to all child fields.
     * When true, the authorization rule also applies to nested fields.
     */
    boolean recursive() default false;

    /**
     * Comma-separated list of operations this rule applies to.
     * If not specified, rule applies to all operations (read, create, update, delete).
     *
     * Example: "read,delete" means rule only applies to reads and deletes
     */
    String operations() default "";

    /**
     * Whether to cache authorization decisions.
     * Useful for expensive rules that don't change frequently.
     */
    boolean cacheable() default true;

    /**
     * Cache duration in seconds (0 = no caching).
     * Only used if cacheable is true.
     */
    int cacheDurationSeconds() default 300;
}
