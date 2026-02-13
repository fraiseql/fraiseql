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
}
