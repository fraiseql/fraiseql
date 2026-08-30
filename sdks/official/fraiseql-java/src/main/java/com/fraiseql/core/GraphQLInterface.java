package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks an interface as a GraphQL interface type.
 * GraphQL interface types define a set of fields that must be implemented by types.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLInterface
 * public interface Node {
 *     &#64;GraphQLField
 *     String getId();
 *
 *     &#64;GraphQLField
 *     String getCreatedAt();
 * }
 *
 * &#64;GraphQLType
 * public class User implements Node {
 *     &#64;GraphQLField
 *     public String id;
 *
 *     &#64;GraphQLField
 *     public String createdAt;
 *
 *     &#64;GraphQLField
 *     public String email;
 * }
 * </pre>
 *
 * <p>Or with builder:
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
