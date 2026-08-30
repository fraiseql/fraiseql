package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks an enum class as a GraphQL enum type.
 * Enum values are automatically extracted from the Java enum constants.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLEnum
 * public enum OrderStatus {
 *     &#64;GraphQLEnumValue("PENDING")
 *     PENDING,
 *     &#64;GraphQLEnumValue("SHIPPED")
 *     SHIPPED,
 *     &#64;GraphQLEnumValue("DELIVERED")
 *     DELIVERED
 * }
 * </pre>
 *
 * <p>Or with values map:
 * <pre>
 * FraiseQL.enum_("OrderStatus", new LinkedHashMap&lt;String, Object&gt;() {{
 *     put("PENDING", "pending");
 *     put("SHIPPED", "shipped");
 *     put("DELIVERED", "delivered");
 * }});
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLEnum {
    /**
     * Optional custom name for the GraphQL enum.
     * If not specified, the Java enum class name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL enum.
     */
    String description() default "";
}
