package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a class as a GraphQL union type.
 * Union types can be one of several specified types.
 * The union class itself is abstract and cannot be instantiated.
 *
 * Usage:
 * <pre>
 * @GraphQLUnion(members = {User.class, Bot.class, Guest.class})
 * public abstract class Actor {
 * }
 *
 * @GraphQLType
 * public class User {
 *     @GraphQLField
 *     public String id;
 *
 *     @GraphQLField
 *     public String name;
 * }
 *
 * @GraphQLType
 * public class Bot {
 *     @GraphQLField
 *     public String id;
 *
 *     @GraphQLField
 *     public String name;
 * }
 * </pre>
 *
 * Or with builder:
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
