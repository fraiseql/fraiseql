package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL fact table support in FraiseQL Java.
 * Fact tables are the core of OLAP (Online Analytical Processing) schemas.
 */
@DisplayName("Fact Tables for Analytics")
public class FactTableTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // BASIC FACT TABLE REGISTRATION
    // =========================================================================

    @Test
    @DisplayName("Register simple fact table")
    void testRegisterSimpleFactTable() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());
        assertEquals("SalesFactTable", typeInfo.get().name);
    }

    @Test
    @DisplayName("Fact table with measures and dimensions")
    void testFactTableWithMeasuresAndDimensions() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        // Measures: revenue, quantity
        // Dimensions: saleDate, region
        assertEquals(4, fields.size());
    }

    @Test
    @DisplayName("Fact table with description")
    void testFactTableWithDescription() {
        FraiseQL.registerType(OrdersFactTable.class);

        var typeInfo = registry.getType("OrdersFactTable");
        assertTrue(typeInfo.isPresent());
    }

    // =========================================================================
    // FACT TABLE WITH MEASURES
    // =========================================================================

    @Test
    @DisplayName("Fact table with multiple measures")
    void testFactTableWithMultipleMeasures() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.containsKey("revenue"));
        assertTrue(fields.containsKey("quantity"));
    }

    @Test
    @DisplayName("Fact table with various measure aggregations")
    void testFactTableWithVariousMeasures() {
        FraiseQL.registerType(MetricsFactTable.class);

        var typeInfo = registry.getType("MetricsFactTable");
        assertTrue(typeInfo.isPresent());

        // Should have count, sum, average measures
        assertTrue(typeInfo.get().fields.size() > 0);
    }

    // =========================================================================
    // FACT TABLE WITH DIMENSIONS
    // =========================================================================

    @Test
    @DisplayName("Fact table with temporal dimension")
    void testFactTableWithTemporalDimension() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());

        assertTrue(typeInfo.get().fields.containsKey("saleDate"));
    }

    @Test
    @DisplayName("Fact table with geographic dimension")
    void testFactTableWithGeographicDimension() {
        FraiseQL.registerType(SalesFactTable.class);

        var typeInfo = registry.getType("SalesFactTable");
        assertTrue(typeInfo.isPresent());

        assertTrue(typeInfo.get().fields.containsKey("region"));
    }

    @Test
    @DisplayName("Fact table with categorical dimension")
    void testFactTableWithCategoricalDimension() {
        FraiseQL.registerType(OrdersFactTable.class);

        var typeInfo = registry.getType("OrdersFactTable");
        assertTrue(typeInfo.isPresent());

        var fields = typeInfo.get().fields;
        assertTrue(fields.size() > 0);
    }

    // =========================================================================
    // MULTIPLE FACT TABLES
    // =========================================================================

    @Test
    @DisplayName("Register multiple fact tables")
    void testRegisterMultipleFactTables() {
        FraiseQL.registerTypes(SalesFactTable.class, OrdersFactTable.class);

        assertTrue(registry.getType("SalesFactTable").isPresent());
        assertTrue(registry.getType("OrdersFactTable").isPresent());
    }

    // =========================================================================
    // AGGREGATE QUERIES ON FACT TABLES
    // =========================================================================

    @Test
    @DisplayName("Aggregate query on fact table")
    void testAggregateQueryOnFactTable() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("salesTotal")
            .returnType("SalesAggregate")
            .register();

        var query = registry.getQuery("salesTotal");
        assertTrue(query.isPresent());
        assertEquals("SalesAggregate", query.get().returnType);
    }

    @Test
    @DisplayName("Aggregate query with dimension grouping")
    void testAggregateQueryWithDimensionGrouping() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("salesByRegion")
            .returnType("RegionalSales")
            .returnsArray(true)
            .arg("region", "String")
            .register();

        var query = registry.getQuery("salesByRegion");
        assertTrue(query.isPresent());
        assertEquals("[RegionalSales]", query.get().returnType);
        assertEquals(1, query.get().arguments.size());
    }

    // =========================================================================
    // FACT TABLE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Star schema with fact and dimensions")
    void testStarSchemaPattern() {
        // Fact table with measures and dimensions
        FraiseQL.registerType(SalesFactTable.class);

        // Dimension tables (can be regular types)
        FraiseQL.registerType(DateDimension.class);
        FraiseQL.registerType(RegionDimension.class);

        assertEquals(3, registry.getAllTypes().size());
        assertTrue(registry.getType("SalesFactTable").isPresent());
        assertTrue(registry.getType("DateDimension").isPresent());
        assertTrue(registry.getType("RegionDimension").isPresent());
    }

    @Test
    @DisplayName("Pattern: Time series aggregation")
    void testTimeSeriesAggregationPattern() {
        FraiseQL.registerType(SalesFactTable.class);

        // Daily sales aggregation
        FraiseQL.query("dailySales")
            .returnType("DailySalesAggregate")
            .returnsArray(true)
            .arg("startDate", "String")
            .arg("endDate", "String")
            .register();

        var query = registry.getQuery("dailySales");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Multi-dimensional aggregation")
    void testMultiDimensionalAggregationPattern() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("salesByRegionAndDate")
            .returnType("SalesAggregate")
            .returnsArray(true)
            .arg("region", "String")
            .arg("date", "String")
            .register();

        var query = registry.getQuery("salesByRegionAndDate");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // CLEAR FACT TABLES
    // =========================================================================

    @Test
    @DisplayName("Clear removes fact tables")
    void testClearRemovesFactTables() {
        FraiseQL.registerType(SalesFactTable.class);

        assertTrue(registry.getType("SalesFactTable").isPresent());

        registry.clear();

        assertFalse(registry.getType("SalesFactTable").isPresent());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLFactTable(
        tableName = "tf_sales",
        description = "Sales transactions fact table"
    )
    public static class SalesFactTable {
        @Measure(aggregation = "SUM", description = "Total revenue")
        public float revenue;

        @Measure(aggregation = "SUM", description = "Quantity sold")
        public int quantity;

        @Dimension(name = "date", description = "Sale date")
        public String saleDate;

        @Dimension(name = "region", description = "Geographic region")
        public String region;
    }

    @GraphQLFactTable(
        tableName = "tf_orders",
        description = "Orders fact table"
    )
    public static class OrdersFactTable {
        @Measure(aggregation = "SUM", description = "Order total")
        public float total;

        @Measure(aggregation = "COUNT", description = "Number of orders")
        public long orderCount;

        @Dimension(name = "orderDate", description = "Order date")
        public String orderDate;

        @Dimension(name = "customer", description = "Customer ID")
        public String customerId;
    }

    @GraphQLFactTable(tableName = "tf_metrics")
    public static class MetricsFactTable {
        @Measure(aggregation = "COUNT", description = "Event count")
        public long eventCount;

        @Measure(aggregation = "AVG", description = "Average duration")
        public float averageDuration;

        @Measure(aggregation = "SUM", description = "Total value")
        public float totalValue;

        @Dimension(name = "timestamp", description = "Event timestamp")
        public String timestamp;
    }

    @GraphQLType
    public static class DateDimension {
        @GraphQLField
        public int year;

        @GraphQLField
        public int month;

        @GraphQLField
        public int day;
    }

    @GraphQLType
    public static class RegionDimension {
        @GraphQLField
        public String region;

        @GraphQLField
        public String country;

        @GraphQLField
        public String continent;
    }

    @GraphQLType
    public static class SalesAggregate {
        @GraphQLField
        public float totalRevenue;

        @GraphQLField
        public int totalQuantity;
    }

    @GraphQLType
    public static class RegionalSales {
        @GraphQLField
        public String region;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public int quantity;
    }

    @GraphQLType
    public static class DailySalesAggregate {
        @GraphQLField
        public String date;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public int transactions;
    }
}
