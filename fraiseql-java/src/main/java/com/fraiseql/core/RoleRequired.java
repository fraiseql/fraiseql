package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field or type as requiring specific user roles for access.
 * Enables role-based access control (RBAC) with flexible matching strategies.
 *
 * Usage:
 * <pre>
 * @GraphQLType
 * @RoleRequired(roles = "admin")
 * public class SystemSettings {
 *     @GraphQLField
 *     public String databaseUrl;
 * }
 *
 * @GraphQLType
 * public class User {
 *     @GraphQLField
 *     public String id;
 *
 *     @GraphQLField
 *     @RoleRequired(roles = {"manager", "admin"}, strategy = RoleMatchStrategy.ANY)
 *     public float salary;
 * }
 *
 * // Multiple role requirements
 * @GraphQLType
 * @RoleRequired(roles = {"compliance_officer", "auditor"}, description = "Requires compliance role")
 * public class ComplianceReport {
 *     @GraphQLField
 *     public String auditTrail;
 * }
 *
 * // Hierarchical roles
 * @GraphQLField
 * @RoleRequired(roles = "manager", hierarchy = true)
 * public float budgetAmount;  // Accessible by 'manager' and any role higher in hierarchy
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.FIELD})
public @interface RoleRequired {
    /**
     * Single role or comma-separated list of required roles.
     * Examples: "admin", "manager,director", "analyst"
     */
    String[] roles() default {};

    /**
     * Strategy for matching multiple roles.
     * - ANY: User must have at least one of the specified roles
     * - ALL: User must have all of the specified roles
     * - EXACTLY: User must have exactly these roles (no more, no less)
     */
    RoleMatchStrategy strategy() default RoleMatchStrategy.ANY;

    /**
     * Whether roles form a hierarchy (e.g., admin > manager > employee).
     * When true, users with higher roles automatically have access.
     * The role hierarchy must be defined elsewhere in the system.
     */
    boolean hierarchy() default false;

    /**
     * Description of what roles are required and why.
     *
     * Example: "Requires manager or higher roles to view sensitive salary data"
     */
    String description() default "";

    /**
     * Custom error message when role requirement is not met.
     *
     * Example: "You must have the manager role to access this"
     */
    String errorMessage() default "";

    /**
     * Comma-separated list of operations this rule applies to.
     * If not specified, rule applies to all operations.
     *
     * Example: "read" = only applied to read operations
     * Example: "create,update,delete" = not applied to reads
     */
    String operations() default "";

    /**
     * Whether to inherit role requirements from parent types.
     * When true, a field's role requirements are combined with its type's requirements.
     */
    boolean inherit() default true;

    /**
     * Whether to cache role validation results.
     */
    boolean cacheable() default true;

    /**
     * Cache duration in seconds for role validation results.
     */
    int cacheDurationSeconds() default 600;

    /**
     * Enum for role matching strategies.
     */
    enum RoleMatchStrategy {
        /**
         * User must have at least one of the specified roles.
         */
        ANY,

        /**
         * User must have all of the specified roles.
         */
        ALL,

        /**
         * User must have exactly these roles.
         */
        EXACTLY
    }
}
