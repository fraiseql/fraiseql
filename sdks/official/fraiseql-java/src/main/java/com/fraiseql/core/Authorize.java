package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field or type with a custom authorization rule.
 * Rules are CEL-like expressions evaluated at runtime by the FraiseQL security layer.
 *
 * <p>Examples:
 * <pre>
 * {@literal @}Authorize(rule = "hasRole($context, 'admin')")
 * public String adminField;
 *
 * {@literal @}Authorize(rule = "isOwner($context.userId, $field.ownerId)")
 * public class ProtectedResource {}
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.FIELD, ElementType.TYPE})
public @interface Authorize {
    /** CEL-like authorization rule expression. */
    String rule() default "";

    /** Reference to a named policy defined via {@code @AuthzPolicy}. */
    String policy() default "";

    /** Human-readable description of this authorization rule. */
    String description() default "";

    /** Whether the rule propagates recursively to nested types. */
    boolean recursive() default false;

    /** Comma-separated operations this rule applies to (e.g. "read", "write", "delete"). */
    String operations() default "";

    /** Custom error message shown when access is denied. */
    String errorMessage() default "";
}
