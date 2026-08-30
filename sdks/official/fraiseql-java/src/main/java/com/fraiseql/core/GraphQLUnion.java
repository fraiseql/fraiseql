package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a class as a GraphQL union type.
 * Union types can be one of several specified types.
 * The union class itself is abstract and cannot be instantiated.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLUnion(members = {User.class, Bot.class, Guest.class})
 * public abstract class Actor {
 * }
 *
 * &#64;GraphQLType
 * public class User {
 *     &#64;GraphQLField
 *     public String id;
 *
 *     &#64;GraphQLField
 *     public String name;
 * }
 *
 * &#64;GraphQLType
 * public class Bot {
 *     &#64;GraphQLField
 *     public String id;
 *
 *     &#64;GraphQLField
 *     public String name;
 * }
 * </pre>
 *
 * <p>Or with builder:
 * <pre>
 * FraiseQL.union("SearchResult", new String[]{"User", "Post", "Comment"});
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLUnion {
    /**
     * The member types that make up this union.
     * Must be classes annotated with @GraphQLType.
     */
    Class<?>[] members() default {};

    /**
     * Optional custom name for the GraphQL union.
     * If not specified, the Java class name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL union.
     */
    String description() default "";
}
