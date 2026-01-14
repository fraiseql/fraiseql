package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field as a GraphQL field within a GraphQL type.
 * Automatically extracts type information from the field's Java type.
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
}
