package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field in a fact table as an aggregatable measure.
 * Measures are numeric values that can be aggregated (SUM, COUNT, AVG, etc.).
 *
 * <p>Example:
 * <pre>
 * {@literal @}GraphQLType
 * public class SalesFactTable {
 *     {@literal @}GraphQLField
 *     {@literal @}Measure(aggregation = "SUM", description = "Total revenue")
 *     public double revenue;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface Measure {
    /** Aggregation function (SUM, COUNT, AVG, MIN, MAX). */
    String aggregation();

    /** Human-readable description of this measure. */
    String description() default "";

    /** Optional unit label (e.g. "USD", "ms"). */
    String unit() default "";
}
