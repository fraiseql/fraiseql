package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a class as a GraphQL fact table type.
 * Fact tables contain metrics (measures) and dimensional data for analytics.
 *
 * Fact tables are used in OLAP (Online Analytical Processing) scenarios where:
 * - Measures represent aggregatable metrics (revenue, quantity, count, etc.)
 * - Dimensions provide categorical slicing (date, region, product, etc.)
 * - Denormalized dimensions optimize query performance
 *
 * Usage:
 * <pre>
 * @GraphQLFactTable(
 *     tableName = "tf_sales",
 *     description = "Sales transactions fact table"
 * )
 * public class SalesFactTable {
 *     @Measure(aggregation = "SUM", description = "Total revenue")
 *     public float revenue;
 *
 *     @Measure(aggregation = "SUM", description = "Quantity sold")
 *     public int quantity;
 *
 *     @Dimension(name = "date", description = "Sale date")
 *     public String saleDate;
 *
 *     @Dimension(name = "region", jsonPath = "data->>'region'")
 *     public String region;
 * }
 *
 * // Register in schema
 * FraiseQL.registerFactTable(SalesFactTable.class);
 *
 * // Create aggregate queries
 * FraiseQL.aggregateQuery("salesByRegion")
 *     .factTable("tf_sales")
 *     .groupBy("region")
 *     .register();
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface GraphQLFactTable {
    /**
     * The database table name for this fact table.
     * For example: "tf_sales", "fact_orders", "metrics_events"
     */
    String tableName();

    /**
     * Optional custom name for the GraphQL fact table type.
     * If not specified, the Java class name is used.
     */
    String name() default "";

    /**
     * Optional description for the GraphQL fact table.
     * Should explain what metrics and dimensions this fact table contains.
     */
    String description() default "";

    /**
     * Whether this fact table supports denormalized filters.
     * Denormalized filters optimize query performance by allowing
     * aggregation on pre-calculated or stored columns.
     */
    boolean supportsDenormalizedFilters() default false;

    /**
     * Primary grain (level of detail) for this fact table.
     * For example: "daily", "hourly", "transaction"
     * Helps query planners understand the finest granularity available.
     */
    String grain() default "";
}
