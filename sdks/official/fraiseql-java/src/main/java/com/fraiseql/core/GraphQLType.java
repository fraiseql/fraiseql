package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a class as a GraphQL type for FraiseQL schema authoring.
 *
 * Usage:
 * <pre>
 * @GraphQLType
 * public class User {
 *     @GraphQLField
 *     public int id;
 *
 *     @GraphQLField
 *     public String name;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLType {
    /**
     * Optional custom name for the GraphQL type.
     * If not specified, the class name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL type.
     */
    String description() default "";

    /**
     * Whether this type implements the Relay Node interface.
     * When true, the type participates in global object identification via node(id: ID!)
     * and can be used as the return type of relay-enabled list queries.
     * Requires pk_{entity} (BIGINT) to be present in the view's data JSONB.
     */
    boolean relay() default false;
}
