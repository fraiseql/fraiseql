package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a field as a GraphQL field within a GraphQL type.
 * Automatically extracts type information from the field's Java type.
 *
 * <p>Supports field-level metadata:
 * - Custom names and types
 * - Field descriptions
 * - Deprecation markers with reasons
 * - JWT scope-based access control
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLType
 * public class User {
 *     &#64;GraphQLField
 *     public int id;
 *
 *     &#64;GraphQLField(nullable = true)
 *     public String email;
 *
 *     &#64;GraphQLField(name = "created_at")
 *     public String createdAt;
 *
 *     &#64;GraphQLField(
 *         deprecated = "Use newEmail instead",
 *         description = "User's old email (deprecated)"
 *     )
 *     public String oldEmail;
 *
 *     &#64;GraphQLField(
 *         requiresScope = "read:user.salary",
 *         description = "User salary (admin only)"
 *     )
 *     public float salary;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.FIELD, ElementType.METHOD})
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
     * <p>Example: "Use newField instead"
     */
    String deprecated() default "";

    /**
     * Optional JWT scope required to access this field.
     * Supports single scope as a string.
     *
     * <p>Example: "read:user.salary"
     */
    String requiresScope() default "\u0000";

    /**
     * Optional JWT scopes required to access this field.
     * Use this for fields that require multiple scopes.
     * All scopes must be present in the user's token to access the field.
     *
     * <p>Example: {"admin", "read:financial"}
     */
    String[] requiresScopes() default {"\u0000"};

    /**
     * Whether this field is computed and should be excluded from CRUD input types.
     * Computed fields are typically auto-generated (like slugs, timestamps, etc.)
     * and should not be set directly by users in create/update operations.
     *
     * <p>When computed=true, the field will be excluded from:
     * - CreateXInput types (all fields)
     * - UpdateXInput types (non-PK fields only)
     *
     * <p>The field remains visible in query results.
     */
    boolean computed() default false;

    /**
     * pgvector configuration, on a {@code Vector} / {@code BitVector} /
     * {@code HalfVector} / {@code SparseVector} field.
     *
     * <p>The compiler refuses such a field without one, so this is what makes the four
     * pgvector field types authorable. Left at its default — {@code dimensions = 0} —
     * the field is not a vector field.
     */
    VectorConfig vector() default @VectorConfig;

    /**
     * On a {@code Float} field, the vector field whose {@code nearest} search distance
     * this field carries.
     *
     * <p>Selecting it on a query that did not run that search is refused, not answered
     * with null: "no distance" and "distance zero" are not the same claim.
     */
    String vectorDistance() default "";
}
