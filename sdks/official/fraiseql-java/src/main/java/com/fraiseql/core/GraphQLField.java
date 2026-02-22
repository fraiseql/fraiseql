package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field as a GraphQL field within a GraphQL type.
 * Automatically extracts type information from the field's Java type.
 *
 * Supports field-level metadata:
 * - Custom names and types
 * - Field descriptions
 * - Deprecation markers with reasons
 * - JWT scope-based access control
 *
 * Usage:
 * <pre>
 * @GraphQLType
 * public class User {
 *     @GraphQLField
 *     public int id;
 *
 *     @GraphQLField(nullable = true)
 *     public String email;
 *
 *     @GraphQLField(name = "created_at")
 *     public String createdAt;
 *
 *     @GraphQLField(
 *         deprecated = "Use newEmail instead",
 *         description = "User's old email (deprecated)"
 *     )
 *     public String oldEmail;
 *
 *     @GraphQLField(
 *         requiresScope = "read:user.salary",
 *         description = "User salary (admin only)"
 *     )
 *     public float salary;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface GraphQLField {
    /**
     * Optional custom name for the GraphQL field.
     * If not specified, the Java field name is used.
     */
    String name() default "";

    /**
     * Whether this field can be null.
     * Nullable fields are represented as Optional&lt;T&gt; in Java.
     */
    boolean nullable() default false;

    /**
     * Optional description for the GraphQL field.
     */
    String description() default "";

    /**
     * Optional custom GraphQL type name.
     * If not specified, the type is inferred from the Java field type.
     */
    String type() default "";

    /**
     * Optional deprecation reason.
     * If set (non-empty), this field is marked as deprecated.
     * The value should explain why it's deprecated and suggest alternatives.
     *
     * Example: "Use newField instead"
     */
    String deprecated() default "";

    /**
     * Optional JWT scope required to access this field.
     * Supports single scope as a string.
     *
     * Example: "read:user.salary"
     */
    String requiresScope() default "";

    /**
     * Optional JWT scopes required to access this field.
     * Use this for fields that require multiple scopes.
     * All scopes must be present in the user's token to access the field.
     *
     * Example: {"admin", "read:financial"}
     */
    String[] requiresScopes() default {};
}
