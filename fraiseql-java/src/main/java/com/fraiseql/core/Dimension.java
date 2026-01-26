package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field as a GraphQL dimension in a fact table.
 * Dimensions provide categorical/hierarchical slicing of measures.
 *
 * Dimensions allow grouping and filtering of measures by:
 * - Temporal hierarchies (date -> month -> quarter -> year)
 * - Geographic hierarchies (city -> state -> region -> country)
 * - Product hierarchies (product -> category -> subcategory)
 * - Organizational hierarchies (employee -> department -> division)
 *
 * Usage:
 * <pre>
 * @GraphQLFactTable(tableName = "tf_sales")
 * public class SalesFactTable {
 *     @Measure(aggregation = "SUM")
 *     public float revenue;
 *
 *     @Dimension(name = "date", description = "Sale date")
 *     public String saleDate;
 *
 *     @Dimension(
 *         name = "region",
 *         jsonPath = "data->>'region'",
 *         description = "Geographic region"
 *     )
 *     public String region;
 *
 *     @Dimension(
 *         name = "product_category",
 *         hierarchy = "product > category > subcategory"
 *     )
 *     public String productCategory;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface Dimension {
    /**
     * The name of this dimension in the GraphQL schema.
     * If not specified, the Java field name is used.
     *
     * Example: "date", "region", "product_category"
     */
    String name() default "";

    /**
     * Optional description of this dimension.
     * Should explain the categorical values and hierarchy.
     *
     * Example: "Geographic sales region (North, South, East, West)"
     */
    String description() default "";

    /**
     * Optional SQL column name (if different from field name).
     * By default, the Java field name is converted to snake_case.
     */
    String sqlName() default "";

    /**
     * Optional JSON path for denormalized dimensions.
     * Used when the dimension is stored in a JSON column.
     *
     * Example: "data->>'region'"
     */
    String jsonPath() default "";

    /**
     * Optional hierarchy definition for multi-level dimensions.
     * Describes the drill-down path from coarse to fine grain.
     *
     * Example: "year > quarter > month > day"
     * Example: "country > state > city"
     */
    String hierarchy() default "";

    /**
     * Optional primary hierarchy level name.
     * Specifies the default level for aggregation.
     *
     * Example: "month" (for a date dimension with year > month > day)
     */
    String primaryHierarchyLevel() default "";

    /**
     * Whether this dimension supports slowly changing dimensions (SCD).
     * Needed for tracking dimensional attribute changes over time.
     */
    boolean isSlowlyChanging() default false;

    /**
     * Optional list of conformed dimension names this dimension conforms to.
     * Conformed dimensions are shared across multiple fact tables.
     *
     * Example: {"date", "geography"}
     */
    String[] conformedDimensions() default {};

    /**
     * Whether this dimension is indexed for query performance.
     */
    boolean indexed() default false;

    /**
     * The cardinality (approximate number of distinct values).
     * Helps query planners choose optimal execution strategies.
     *
     * Example: 365 (for daily dates in a year)
     */
    int cardinality() default -1;
}
