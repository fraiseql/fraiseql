package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for GraphQL aggregate query support on fact tables.
 * Aggregate queries enable OLAP analysis with grouped measures.
 */
@DisplayName("Aggregate Queries")
public class AggregateQueryTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // BASIC AGGREGATE QUERY REGISTRATION
    // =========================================================================

    @Test
    @DisplayName("Register simple aggregate query")
    void testRegisterSimpleAggregateQuery() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("totalSales")
            .returnType("SalesAggregate")
            .register();

        var query = registry.getQuery("totalSales");
        assertTrue(query.isPresent());
        assertEquals("SalesAggregate", query.get().returnType);
    }

    @Test
    @DisplayName("Aggregate query returning array")
    void testAggregateQueryReturnsArray() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("salesByRegion")
            .returnType("RegionalSales")
            .returnsArray(true)
            .register();

        var query = registry.getQuery("salesByRegion");
        assertTrue(query.isPresent());
        assertEquals("[RegionalSales]", query.get().returnType);
    }

    @Test
    @DisplayName("Aggregate query with dimension argument")
    void testAggregateQueryWithDimensionArgument() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("salesByRegion")
            .returnType("RegionalSales")
            .returnsArray(true)
            .arg("region", "String")
            .register();

        var query = registry.getQuery("salesByRegion");
        assertTrue(query.isPresent());
        assertEquals(1, query.get().arguments.size());
        assertTrue(query.get().arguments.containsKey("region"));
    }

    // =========================================================================
    // MULTIPLE DIMENSION AGGREGATION
    // =========================================================================

    @Test
    @DisplayName("Aggregate query with multiple dimensions")
    void testAggregateQueryWithMultipleDimensions() {
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

    @Test
    @DisplayName("Aggregate query with dimension filters")
    void testAggregateQueryWithDimensionFilters() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("filterSales")
            .returnType("SalesAggregate")
            .returnsArray(true)
            .arg("minRevenue", "Float")
            .arg("maxRevenue", "Float")
            .arg("region", "String")
            .register();

        var query = registry.getQuery("filterSales");
        assertTrue(query.isPresent());
        assertEquals(3, query.get().arguments.size());
    }

    // =========================================================================
    // AGGREGATE QUERY PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Total aggregation (no dimensions)")
    void testTotalAggregationPattern() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("totalRevenue")
            .returnType("TotalSales")
            .description("Overall total revenue")
            .register();

        var query = registry.getQuery("totalRevenue");
        assertTrue(query.isPresent());
        assertEquals(0, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Time series aggregation")
    void testTimeSeriesAggregationPattern() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("dailySales")
            .returnType("DailySalesData")
            .returnsArray(true)
            .arg("startDate", "String")
            .arg("endDate", "String")
            .register();

        var query = registry.getQuery("dailySales");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Geographic aggregation")
    void testGeographicAggregationPattern() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("salesByCountry")
            .returnType("CountrySales")
            .returnsArray(true)
            .arg("year", "Int")
            .register();

        var query = registry.getQuery("salesByCountry");
        assertTrue(query.isPresent());
        assertEquals(1, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Cohort analysis")
    void testCohortAnalysisPattern() {
        FraiseQL.registerType(EventFactTable.class);

        FraiseQL.query("userCohort")
            .returnType("CohortData")
            .returnsArray(true)
            .arg("cohortDate", "String")
            .arg("cohortSize", "Int")
            .register();

        var query = registry.getQuery("userCohort");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // MULTIPLE AGGREGATE QUERIES
    // =========================================================================

    @Test
    @DisplayName("Multiple aggregate queries on same fact table")
    void testMultipleAggregateQueriesOnSameFactTable() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("totalSales").returnType("SalesAggregate").register();
        FraiseQL.query("salesByRegion")
            .returnType("RegionalSales")
            .returnsArray(true)
            .arg("region", "String")
            .register();
        FraiseQL.query("salesByDate")
            .returnType("DailySalesData")
            .returnsArray(true)
            .arg("date", "String")
            .register();

        assertEquals(3, registry.getAllQueries().size());
        assertTrue(registry.getQuery("totalSales").isPresent());
        assertTrue(registry.getQuery("salesByRegion").isPresent());
        assertTrue(registry.getQuery("salesByDate").isPresent());
    }

    // =========================================================================
    // AGGREGATE QUERY WITH FILTERS
    // =========================================================================

    @Test
    @DisplayName("Aggregate query with measure filters")
    void testAggregateQueryWithMeasureFilters() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("highValueSales")
            .returnType("SalesAggregate")
            .returnsArray(true)
            .arg("minRevenue", "Float")
            .arg("minQuantity", "Int")
            .register();

        var query = registry.getQuery("highValueSales");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    @Test
    @DisplayName("Aggregate query with date range filter")
    void testAggregateQueryWithDateRangeFilter() {
        FraiseQL.registerType(SalesFactTable.class);

        FraiseQL.query("quarterSales")
            .returnType("SalesAggregate")
            .returnsArray(true)
            .arg("startDate", "String")
            .arg("endDate", "String")
            .arg("region", "String")
            .register();

        var query = registry.getQuery("quarterSales");
        assertTrue(query.isPresent());
        assertEquals(3, query.get().arguments.size());
    }

    // =========================================================================
    // CLEAR AGGREGATE QUERIES
    // =========================================================================

    @Test
    @DisplayName("Clear removes aggregate queries")
    void testClearRemovesAggregateQueries() {
        FraiseQL.registerType(SalesFactTable.class);
        FraiseQL.query("totalSales").returnType("SalesAggregate").register();

        assertTrue(registry.getQuery("totalSales").isPresent());

        registry.clear();

        assertFalse(registry.getQuery("totalSales").isPresent());
    }

    // =========================================================================
    // TEST FIXTURES
    // =========================================================================

    @GraphQLFactTable(tableName = "tf_sales")
    public static class SalesFactTable {
        @Measure(aggregation = "SUM", description = "Total revenue")
        public float revenue;

        @Measure(aggregation = "SUM", description = "Quantity sold")
        public int quantity;

        @Dimension(name = "date", description = "Sale date")
        public String date;

        @Dimension(name = "region", description = "Geographic region")
        public String region;
    }

    @GraphQLFactTable(tableName = "tf_events")
    public static class EventFactTable {
        @Measure(aggregation = "COUNT", description = "Event count")
        public long eventCount;

        @Measure(aggregation = "AVG", description = "Average event value")
        public float averageValue;

        @Dimension(name = "timestamp", description = "Event timestamp")
        public String timestamp;

        @Dimension(name = "category", description = "Event category")
        public String category;
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
    public static class TotalSales {
        @GraphQLField
        public float totalRevenue;
    }

    @GraphQLType
    public static class DailySalesData {
        @GraphQLField
        public String date;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public int transactions;
    }

    @GraphQLType
    public static class CountrySales {
        @GraphQLField
        public String country;

        @GraphQLField
        public float revenue;
    }

    @GraphQLType
    public static class CohortData {
        @GraphQLField
        public String cohort;

        @GraphQLField
        public int size;

        @GraphQLField
        public float retention;
    }
}
