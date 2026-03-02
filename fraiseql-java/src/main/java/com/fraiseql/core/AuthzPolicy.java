package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Defines a reusable authorization policy that can be referenced by name in {@code @Authorize}.
 *
 * <p>Example:
 * <pre>
 * {@literal @}AuthzPolicy(
 *     name = "adminOnly",
 *     type = AuthzPolicy.AuthzPolicyType.RBAC,
 *     rule = "hasRole($context, 'admin')"
 * )
 * public class AdminPolicy {}
 *
 * {@literal @}GraphQLType
 * public class SecureData {
 *     {@literal @}GraphQLField
 *     {@literal @}Authorize(policy = "adminOnly")
 *     public String sensitiveField;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface AuthzPolicy {
    /** Unique policy name for reference via {@code @Authorize(policy = "...")}. */
    String name();

    /** Human-readable description of this policy. */
    String description() default "";

    /** The type of authorization policy. */
    AuthzPolicyType type() default AuthzPolicyType.CUSTOM;

    /** CEL-like rule expression (for RBAC, CUSTOM, HYBRID policies). */
    String rule() default "";

    /** ABAC attribute expressions (e.g. "clearance_level >= 2"). */
    String[] attributes() default {};

    /** Whether the policy propagates recursively to nested types. */
    boolean recursive() default false;

    /** Comma-separated operations this policy applies to. */
    String operations() default "";

    /** Whether the authorization result can be cached. */
    boolean cacheable() default false;

    /** Cache duration in seconds when {@code cacheable = true}. */
    int cacheDurationSeconds() default 0;

    /** Whether access decisions should be written to the audit log. */
    boolean auditLogging() default false;

    /**
     * Authorization policy type.
     */
    enum AuthzPolicyType {
        /** Role-Based Access Control. */
        RBAC,
        /** Attribute-Based Access Control. */
        ABAC,
        /** Hybrid RBAC + ABAC. */
        HYBRID,
        /** Custom rule expression. */
        CUSTOM
    }
}
