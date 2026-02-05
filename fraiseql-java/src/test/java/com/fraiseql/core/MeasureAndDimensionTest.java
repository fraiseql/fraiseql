package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL measure and dimension support in fact tables.
 * Measures are aggregatable metrics; dimensions provide categorical slicing.
 */
@DisplayName("Measures and Dimensions")
public class MeasureAndDimensionTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // MEASURE TESTS
    // =========================================================================

    @Test
    @DisplayName("Fact table with SUM measure")
    void testSumMeasure() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());

        // Revenue field is a SUM measure
        assertTrue(typeInfo.get().fields.containsKey("revenue"));
    }

    @Test
    @DisplayName("Fact table with COUNT measure")
    void testCountMeasure() {
        FraiseQL.registerType(EventFactTable.class);

        var typeInfo = registry.getType("EventFactTable");
        assertTrue(typeInfo.isPresent());

        assertTrue(typeInfo.get().fields.containsKey("eventCount"));
    }

    @Test
    @DisplayName("Fact table with AVG measure")
    void testAverageMeasure() {
        FraiseQL.registerType(PerformanceFactTable.class);

        var typeInfo = registry.getType("PerformanceFactTable");
        assertTrue(typeInfo.isPresent());

        assertTrue(typeInfo.get().fields.containsKey("averageResponseTime"));
    }

    @Test
    @DisplayName("Fact table with multiple measures of different types")
    void testMultipleMeasureTypes() {
        FraiseQL.registerType(ComprehensiveFactTable.class);

        var typeInfo = registry.getType("ComprehensiveFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(5, fields.size());
        assertTrue(fields.containsKey("revenue"));
        assertTrue(fields.containsKey("quantity"));
        assertTrue(fields.containsKey("transactionCount"));
        assertTrue(fields.containsKey("averagePrice"));
        assertTrue(fields.containsKey("date"));
    }

    // =========================================================================
    // DIMENSION TESTS
    // =========================================================================

    @Test
    @DisplayName("Fact table with temporal dimension")
    void testTemporalDimension() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());

        assertTrue(typeInfo.get().fields.containsKey("date"));
    }

    @Test
    @DisplayName("Fact table with geographic dimension")
    void testGeographicDimension() {
        FraiseQL.registerType(GeoFactTable.class);

        var typeInfo = registry.getType("GeoFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("region"));
        assertTrue(fields.containsKey("country"));
    }

    @Test
    @DisplayName("Fact table with categorical dimension")
    void testCategoricalDimension() {
        FraiseQL.registerType(ProductFactTable.class);

        var typeInfo = registry.getType("ProductFactTable");
        assertTrue(typeInfo.isPresent());

        assertTrue(typeInfo.get().fields.containsKey("category"));
    }

    @Test
    @DisplayName("Fact table with multiple dimensions")
    void testMultipleDimensions() {
        FraiseQL.registerType(MultiDimensionFactTable.class);

        var typeInfo = registry.getType("MultiDimensionFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(5, fields.size());
        assertTrue(fields.containsKey("revenue"));
        assertTrue(fields.containsKey("date"));
        assertTrue(fields.containsKey("region"));
        assertTrue(fields.containsKey("category"));
        assertTrue(fields.containsKey("customer"));
    }

    // =========================================================================
    // DENORMALIZED DIMENSION TESTS
    // =========================================================================

    @Test
    @DisplayName("Dimension with JSON path for denormalized data")
    void testDenormalizedDimension() {
        FraiseQL.registerType(DenormalizedFactTable.class);

        var typeInfo = registry.getType("DenormalizedFactTable");
        assertTrue(typeInfo.isPresent());

        // Dimensions stored in JSON
        assertTrue(typeInfo.get().fields.containsKey("region"));
        assertTrue(typeInfo.get().fields.containsKey("category"));
    }

    // =========================================================================
    // MEASURE AGGREGATION PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Basic metrics (count, sum, average)")
    void testBasicMetricsPattern() {
        FraiseQL.registerType(BasicMetricsFactTable.class);

        var typeInfo = registry.getType("BasicMetricsFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("transactionCount"));
        assertTrue(fields.containsKey("totalAmount"));
        assertTrue(fields.containsKey("averageAmount"));
    }

    @Test
    @DisplayName("Pattern: Financial metrics")
    void testFinancialMetricsPattern() {
        FraiseQL.registerType(FinancialFactTable.class);

        var typeInfo = registry.getType("FinancialFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("revenue"));
        assertTrue(fields.containsKey("cost"));
        assertTrue(fields.containsKey("profit"));
    }

    @Test
    @DisplayName("Pattern: Performance metrics")
    void testPerformanceMetricsPattern() {
        FraiseQL.registerType(PerformanceFactTable.class);

        var typeInfo = registry.getType("PerformanceFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("averageResponseTime"));
        assertTrue(fields.containsKey("successCount"));
        assertTrue(fields.containsKey("errorCount"));
    }

    // =========================================================================
    // DIMENSION HIERARCHY PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Temporal hierarchy (year-month-day)")
    void testTemporalHierarchyPattern() {
        FraiseQL.registerType(TimeHierarchyFactTable.class);

        var typeInfo = registry.getType("TimeHierarchyFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertEquals(4, fields.size());
        assertTrue(fields.containsKey("revenue"));
        assertTrue(fields.containsKey("year"));
        assertTrue(fields.containsKey("month"));
        assertTrue(fields.containsKey("day"));
    }

    @Test
    @DisplayName("Pattern: Geographic hierarchy (continent-country-region)")
    void testGeographicHierarchyPattern() {
        FraiseQL.registerType(GeoHierarchyFactTable.class);

        var typeInfo = registry.getType("GeoHierarchyFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("revenue"));
        assertTrue(fields.containsKey("continent"));
        assertTrue(fields.containsKey("country"));
        assertTrue(fields.containsKey("region"));
    }

    // =========================================================================
    // CONFORMED DIMENSIONS
    // =========================================================================

    @Test
    @DisplayName("Multiple fact tables with conformed dimensions")
    void testConformedDimensions() {
        FraiseQL.registerTypes(
            SalesFactTable.class,
            InventoryFactTable.class
        );

        assertTrue(registry.getType("SalesFactTable").isPresent());
        assertTrue(registry.getType("InventoryFactTable").isPresent());

        // Both share date and region dimensions
        var salesFields = registry.getType("SalesFactTable").get().fields;
        var inventoryFields = registry.getType("InventoryFactTable").get().fields;

        assertTrue(salesFields.containsKey("date"));
        assertTrue(inventoryFields.containsKey("date"));
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLFactTable(tableName = "tf_sales")
    public static class SalesFactTable {
        @Measure(aggregation = "SUM")
        public float revenue;

        @Dimension(name = "date")
        public String date;
    }

    @GraphQLFactTable(tableName = "tf_events")
    public static class EventFactTable {
        @Measure(aggregation = "COUNT", description = "Event count")
        public long eventCount;

        @Dimension(name = "timestamp")
        public String timestamp;
    }

    @GraphQLFactTable(tableName = "tf_performance")
    public static class PerformanceFactTable {
        @Measure(aggregation = "AVG", description = "Average response time")
        public float averageResponseTime;

        @Measure(aggregation = "COUNT", description = "Success count")
        public long successCount;

        @Measure(aggregation = "COUNT", description = "Error count")
        public long errorCount;

        @Dimension(name = "timestamp")
        public String timestamp;
    }

    @GraphQLFactTable(tableName = "tf_comprehensive")
    public static class ComprehensiveFactTable {
        @Measure(aggregation = "SUM")
        public float revenue;

        @Measure(aggregation = "SUM")
        public int quantity;

        @Measure(aggregation = "COUNT")
        public long transactionCount;

        @Measure(aggregation = "AVG")
        public float averagePrice;

        @Dimension(name = "date")
        public String date;
    }

    @GraphQLFactTable(tableName = "tf_geo")
    public static class GeoFactTable {
        @Measure(aggregation = "SUM")
        public float amount;

        @Dimension(name = "region")
        public String region;

        @Dimension(name = "country")
        public String country;
    }

    @GraphQLFactTable(tableName = "tf_products")
    public static class ProductFactTable {
        @Measure(aggregation = "SUM")
        public float sales;

        @Dimension(name = "category")
        public String category;
    }

    @GraphQLFactTable(tableName = "tf_multidim")
    public static class MultiDimensionFactTable {
        @Measure(aggregation = "SUM")
        public float revenue;

        @Dimension(name = "date")
        public String date;

        @Dimension(name = "region")
        public String region;

        @Dimension(name = "category")
        public String category;

        @Dimension(name = "customer")
        public String customer;
    }

    @GraphQLFactTable(tableName = "tf_denorm")
    public static class DenormalizedFactTable {
        @Measure(aggregation = "SUM")
        public float amount;

        @Dimension(name = "region", jsonPath = "attributes->>'region'")
        public String region;

        @Dimension(name = "category", jsonPath = "attributes->>'category'")
        public String category;
    }

    @GraphQLFactTable(tableName = "tf_basic_metrics")
    public static class BasicMetricsFactTable {
        @Measure(aggregation = "COUNT")
        public long transactionCount;

        @Measure(aggregation = "SUM")
        public float totalAmount;

        @Measure(aggregation = "AVG")
        public float averageAmount;

        @Dimension(name = "date")
        public String date;
    }

    @GraphQLFactTable(tableName = "tf_financial")
    public static class FinancialFactTable {
        @Measure(aggregation = "SUM")
        public float revenue;

        @Measure(aggregation = "SUM")
        public float cost;

        @Measure(aggregation = "SUM")
        public float profit;

        @Dimension(name = "date")
        public String date;
    }

    @GraphQLFactTable(tableName = "tf_time_hierarchy")
    public static class TimeHierarchyFactTable {
        @Measure(aggregation = "SUM")
        public float revenue;

        @Dimension(name = "year", hierarchy = "year > month > day")
        public int year;

        @Dimension(name = "month")
        public int month;

        @Dimension(name = "day")
        public int day;
    }

    @GraphQLFactTable(tableName = "tf_geo_hierarchy")
    public static class GeoHierarchyFactTable {
        @Measure(aggregation = "SUM")
        public float revenue;

        @Dimension(name = "continent", hierarchy = "continent > country > region")
        public String continent;

        @Dimension(name = "country")
        public String country;

        @Dimension(name = "region")
        public String region;
    }

    @GraphQLFactTable(tableName = "tf_inventory")
    public static class InventoryFactTable {
        @Measure(aggregation = "SUM")
        public int quantity;

        @Dimension(name = "date", conformedDimensions = {"sales_date"})
        public String date;

        @Dimension(name = "region", conformedDimensions = {"sales_region"})
        public String region;
    }
}
