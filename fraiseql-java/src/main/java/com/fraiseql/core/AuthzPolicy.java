package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Defines a reusable authorization policy that can be referenced by other decorators.
 * Policies centralize authorization logic for consistency and maintenance.
 *
 * Usage:
 * <pre>
 * // Define a policy
 * @AuthzPolicy(
 *     name = "piiAccess",
 *     description = "Access to Personally Identifiable Information",
 *     rule = "hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')"
 * )
 * public class PIIAccessPolicy {}
 *
 * // Use the policy
 * @GraphQLType
 * public class Customer {
 *     @GraphQLField
 *     public String id;
 *
 *     @GraphQLField
 *     @Authorize(policy = "piiAccess")
 *     public String email;
 *
 *     @GraphQLField
 *     @Authorize(policy = "piiAccess")
 *     public String phoneNumber;
 * }
 *
 * // Policy with attributes
 * @AuthzPolicy(
 *     name = "financialData",
 *     type = AuthzPolicyType.ATTRIBUTE,
 *     attributes = {"clearance_level >= 3", "department == 'finance'"},
 *     description = "Access financial records requires clearance level 3 and finance department"
 * )
 * public class FinancialDataPolicy {}
 *
 * // Policy with multiple conditions
 * @AuthzPolicy(
 *     name = "auditAccess",
 *     type = AuthzPolicyType.HYBRID,
 *     rule = "hasRole($context, 'auditor')",
 *     attributes = {"audit_enabled == true"},
 *     description = "Auditors can view audit trails when audit is enabled"
 * )
 * public class AuditAccessPolicy {}
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface AuthzPolicy {
    /**
     * Unique name for this policy.
     * Used when referencing the policy via @Authorize(policy = "name")
     *
     * Example: "piiAccess", "financialData", "adminOnly"
     */
    String name();

    /**
     * Description of what this policy protects and why.
     *
     * Example: "Restricts access to personally identifiable information"
     */
    String description() default "";

    /**
     * Custom authorization rule expression.
     * Used for role-based and custom logic policies.
     *
     * Example: "hasRole($context, 'admin') OR isOwner($context.userId, $field.ownerId)"
     */
    String rule() default "";

    /**
     * List of attribute conditions to check.
     * Used for attribute-based access control (ABAC).
     *
     * Examples:
     * - "clearance_level >= 3"
     * - "department == 'finance'"
     * - "country IN ('US', 'CA', 'MX')"
     */
    String[] attributes() default {};

    /**
     * Type of authorization policy.
     * - RBAC: Role-based access control
     * - ABAC: Attribute-based access control
     * - CUSTOM: Custom rule expressions
     * - HYBRID: Combination of multiple approaches
     */
    AuthzPolicyType type() default AuthzPolicyType.CUSTOM;

    /**
     * Whether to cache authorization decisions for this policy.
     */
    boolean cacheable() default true;

    /**
     * Cache duration in seconds.
     */
    int cacheDurationSeconds() default 300;

    /**
     * Whether this policy applies hierarchically to child fields.
     */
    boolean recursive() default false;

    /**
     * Operations this policy applies to.
     * If empty, applies to all operations.
     *
     * Example: "read", or "create,update,delete"
     */
    String operations() default "";

    /**
     * Audit logging configuration.
     * Whether to log access attempts for this policy.
     */
    boolean auditLogging() default true;

    /**
     * Error message to return when policy check fails.
     */
    String errorMessage() default "";

    /**
     * Enum for policy types.
     */
    enum AuthzPolicyType {
        /**
         * Role-based access control (RBAC).
         */
        RBAC,

        /**
         * Attribute-based access control (ABAC).
         */
        ABAC,

        /**
         * Custom rule expressions.
         */
        CUSTOM,

        /**
         * Hybrid approach combining multiple methods.
         */
        HYBRID
    }
}
