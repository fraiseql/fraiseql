package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a class as a GraphQL input type.
 * Input types are used as arguments to queries and mutations.
 * Unlike regular types, input types cannot have fields that are lists
 * and all fields must be input-compatible (scalars, enums, or other input types).
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLInput
 * public class CreateUserInput {
 *     &#64;GraphQLField
 *     public String name;
 *
 *     &#64;GraphQLField
 *     public String email;
 *
 *     &#64;GraphQLField(nullable = true)
 *     public String phone;
 *
 *     &#64;GraphQLField(nullable = true)
 *     public String defaultRole;
 * }
 *
 * // Usage in mutation:
 * FraiseQL.mutation("createUser")
 *     .returnType("User")
 *     .arg("input", "CreateUserInput")
 *     .register();
 * </pre>
 *
 * <p>Or with builder:
 * <pre>
 * FraiseQL.input("FilterInput", new Field[]{
 *     new Field("query", "String", false),
 *     new Field("limit", "Int", true, 10)
 * });
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLInput {
    /**
     * Optional custom name for the GraphQL input type.
     * If not specified, the Java class name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL input type.
     */
    String description() default "";
}
