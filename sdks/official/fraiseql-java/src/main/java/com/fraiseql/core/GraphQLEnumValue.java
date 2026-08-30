package com.fraiseql.core;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks an enum constant as a GraphQL enum value.
 * Can optionally specify a custom GraphQL value and description.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLEnum
 * public enum OrderStatus {
 *     &#64;GraphQLEnumValue("PENDING")
 *     PENDING,
 *     &#64;GraphQLEnumValue(value = "SHIPPED", description = "Order has been shipped")
 *     SHIPPED,
 *     &#64;GraphQLEnumValue(value = "DELIVERED")
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
