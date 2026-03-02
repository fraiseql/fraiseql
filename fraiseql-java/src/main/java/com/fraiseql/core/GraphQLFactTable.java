package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a class as a FraiseQL fact table for OLAP analytics.
 * Fact tables combine {@link Measure}s (aggregatable metrics) and
 * {@link Dimension}s (categorical axes).
 *
 * <p>Example:
 * <pre>
 * {@literal @}GraphQLFactTable(tableName = "tf_sales")
 * public class SalesFactTable {
 *     {@literal @}Measure(aggregation = "SUM")
 *     public float revenue;
 *
 *     {@literal @}Dimension(name = "date")
 *     public String saleDate;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLFactTable {
    /** The underlying database table or view name. */
    String tableName();

    /** Optional description of this fact table. */
    String description() default "";

    /** Optional grain specification (e.g. "transaction", "daily"). */
    String grain() default "";
}
