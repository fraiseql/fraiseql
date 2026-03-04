package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks an interface as a GraphQL interface type.
 * GraphQL interface types define a set of fields that must be implemented by types.
 *
 * Usage:
 * <pre>
 * @GraphQLInterface
 * public interface Node {
 *     @GraphQLField
 *     String getId();
 *
 *     @GraphQLField
 *     String getCreatedAt();
 * }
 *
 * @GraphQLType
 * public class User implements Node {
 *     @GraphQLField
 *     public String id;
 *
 *     @GraphQLField
 *     public String createdAt;
 *
 *     @GraphQLField
 *     public String email;
 * }
 * </pre>
 *
 * Or with builder:
 * <pre>
 * FraiseQL.interface_("Node", new Field[]{
 *     new Field("id", "ID", false),
 *     new Field("createdAt", "DateTime", false)
 * });
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLInterface {
    /**
     * Optional custom name for the GraphQL interface.
     * If not specified, the Java interface name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL interface.
     */
    String description() default "";
}
