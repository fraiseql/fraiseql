package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field or type as requiring one or more roles for access.
 *
 * <p>Examples:
 * <pre>
 * {@literal @}RoleRequired(roles = "admin")
 * public String adminField;
 *
 * {@literal @}RoleRequired(roles = {"manager", "hr"}, strategy = RoleRequired.RoleMatchStrategy.ANY)
 * public String hrField;
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.FIELD, ElementType.TYPE})
public @interface RoleRequired {

    /** The role or roles required to access this element. */
    String[] roles();

    /** Strategy for matching multiple roles (default ANY). */
    RoleMatchStrategy strategy() default RoleMatchStrategy.ANY;

    /** Whether role hierarchy should be respected (parent roles inherit child permissions). */
    boolean hierarchy() default false;

    /** Whether nested types inherit this role requirement. */
    boolean inherit() default false;

    /** Comma-separated operations this rule applies to (e.g. "read", "delete"). */
    String operations() default "";

    /** Human-readable description of this role requirement. */
    String description() default "";

    /**
     * Strategy for evaluating multiple required roles.
     */
    enum RoleMatchStrategy {
        /** Access is granted if the user has ANY of the specified roles. */
        ANY,
        /** Access is granted only if the user has ALL of the specified roles. */
        ALL
    }
}
