package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * Marks a field in a fact table as a dimension for categorical slicing.
 * Dimensions provide grouping axes for aggregate queries.
 *
 * <p>Example:
 * <pre>
 * {@literal @}GraphQLType
 * public class SalesFactTable {
 *     {@literal @}GraphQLField
 *     {@literal @}Dimension(name = "date", hierarchy = "year > quarter > month > day")
 *     public String saleDate;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface Dimension {
    /** Logical name for this dimension (may differ from field name). */
    String name() default "";

    /** Human-readable description of this dimension. */
    String description() default "";

    /** Hierarchy expression (e.g. "year > quarter > month > day"). */
    String hierarchy() default "";

    /** Expected cardinality (approximate number of distinct values). */
    int cardinality() default 0;

    /** Optional JSONB path expression for nested dimension extraction. */
    String jsonPath() default "";

    /** Optional list of conformed dimension view names. */
    String[] conformedDimensions() default {};
}
