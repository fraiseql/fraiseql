package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a class as a GraphQL union type: a value that is one of several object types.
 * The class only carries the annotation; it is never instantiated.
 *
 * <p>The annotation is read when the class is registered with
 * {@link FraiseQL#registerUnion(Class)}, as {@code @GraphQLType} is read by
 * {@link FraiseQL#registerType(Class)}. Each member must be a {@code @GraphQLType} class, and
 * is named in the union by its GraphQL type name.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLUnion(members = {User.class, Bot.class}, description = "Who acted")
 * public abstract class Actor {
 * }
 *
 * &#64;GraphQLType
 * public class User {
 *     &#64;GraphQLField
 *     public String id;
 * }
 *
 * &#64;GraphQLType
 * public class Bot {
 *     &#64;GraphQLField
 *     public String id;
 * }
 *
 * FraiseQL.registerTypes(User.class, Bot.class);
 * FraiseQL.registerUnion(Actor.class);
 * </pre>
 *
 * <p>A union whose members are known only by name is registered with
 * {@code FraiseQL.getRegistry().registerUnion("Actor", List.of("User", "Bot"), "Who acted")}.
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
