package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks an enum class as a GraphQL enum type.
 * Enum values are automatically extracted from the Java enum constants.
 *
 * Usage:
 * <pre>
 * @GraphQLEnum
 * public enum OrderStatus {
 *     @GraphQLEnumValue("PENDING")
 *     PENDING,
 *     @GraphQLEnumValue("SHIPPED")
 *     SHIPPED,
 *     @GraphQLEnumValue("DELIVERED")
 *     DELIVERED
 * }
 * </pre>
 *
 * Or with values map:
 * <pre>
 * FraiseQL.enum_("OrderStatus", new LinkedHashMap<String, Object>() {{
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
