package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field as a GraphQL measure (metric) in a fact table.
 * Measures are numeric fields that are aggregatable in analytics queries.
 *
 * Measures represent quantitative business metrics that can be:
 * - Summed (revenue, quantity)
 * - Averaged (price, rating)
 * - Counted (transactions)
 * - Min/Max (temperature, balance)
 *
 * Usage:
 * <pre>
 * @GraphQLFactTable(tableName = "tf_sales")
 * public class SalesFactTable {
 *     @Measure(
 *         aggregation = "SUM",
 *         description = "Total revenue in dollars"
 *     )
 *     public float revenue;
 *
 *     @Measure(
 *         aggregation = "SUM",
 *         description = "Number of items sold"
 *     )
 *     public int quantity;
 *
 *     @Measure(
 *         aggregation = "AVG",
 *         description = "Average transaction amount"
 *     )
 *     public float averageAmount;
 *
 *     @Measure(
 *         aggregation = "COUNT",
 *         description = "Number of transactions"
 *     )
 *     public long transactionCount;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface Measure {
    /**
     * The default aggregation function for this measure.
     * Common values: "SUM", "AVG", "COUNT", "MIN", "MAX", "STDDEV", "VARIANCE"
     *
     * Required unless the measure is non-aggregatable (rare).
     */
    String aggregation() default "SUM";

    /**
     * Optional description of this measure.
     * Should explain the business meaning and unit of measurement.
     *
     * Example: "Total revenue in USD"
     */
    String description() default "";

    /**
     * Optional SQL name for this measure (if different from field name).
     * By default, the Java field name is converted to snake_case.
     */
    String sqlName() default "";

    /**
     * The SQL type of this measure.
     * For example: "DECIMAL(10,2)", "BIGINT", "FLOAT8"
     * If not specified, inferred from Java type.
     */
    String sqlType() default "";

    /**
     * Whether this measure can be null.
     */
    boolean nullable() default false;

    /**
     * Optional unit of measurement.
     * For example: "USD", "units", "percentage", "seconds"
     */
    String unit() default "";

    /**
     * Optional hint for query optimization.
     * For example: "indexed", "compressed", "partitioned"
     */
    String optimization() default "";
}
