package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks an enum constant as a GraphQL enum value.
 * Can optionally specify a custom GraphQL value and description.
 *
 * Usage:
 * <pre>
 * @GraphQLEnum
 * public enum OrderStatus {
 *     @GraphQLEnumValue("PENDING")
 *     PENDING,
 *     @GraphQLEnumValue(value = "SHIPPED", description = "Order has been shipped")
 *     SHIPPED,
 *     @GraphQLEnumValue(value = "DELIVERED")
 *     DELIVERED
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface GraphQLEnumValue {
    /**
     * The GraphQL enum value name.
     * If not specified, the Java enum constant name is used.
     */
    String value() default "";

    /**
     * Optional description for this enum value.
     */
    String description() default "";
}
