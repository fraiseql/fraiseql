package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Tests for advanced OLAP (Online Analytical Processing) patterns in FraiseQL.
 * Demonstrates real-world analytics scenarios using fact tables,
 * measures, and dimensions.
 */
@DisplayName("Analytics Patterns")
public class AnalyticsPatternTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    // =========================================================================
    // CHAINED AGGREGATION PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Chained aggregation (revenue → profit margin)")
    void testChainedAggregationPattern() {
        FraiseQL.registerType(TransactionalFactTable.class);

        // First aggregation: get totals
        FraiseQL.query("salesTotals")
            .returnType("SalesTotals")
            .arg("region", "String")
            .arg("period", "String")
            .register();

        // Second aggregation: calculate margins
        FraiseQL.query("profitMargins")
            .returnType("ProfitMargin")
            .returnsArray(true)
            .arg("minMargin", "Float")
            .arg("region", "String")
            .register();

        var salesQuery = registry.getQuery("salesTotals");
        var marginQuery = registry.getQuery("profitMargins");

        assertTrue(salesQuery.isPresent());
        assertTrue(marginQuery.isPresent());
        assertEquals(2, salesQuery.get().arguments.size());
        assertEquals(2, marginQuery.get().arguments.size());
    }

    // =========================================================================
    // CUMULATIVE MEASURE PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Running total (cumulative revenue)")
    void testRunningTotalPattern() {
        FraiseQL.registerType(TimeSeriesFactTable.class);

        FraiseQL.query("cumulativeRevenue")
            .returnType("CumulativeData")
            .returnsArray(true)
            .arg("startDate", "String")
            .arg("endDate", "String")
            .arg("region", "String")
            .register();

        var query = registry.getQuery("cumulativeRevenue");
        assertTrue(query.isPresent());
        assertEquals(3, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Moving average (smoothed trends)")
    void testMovingAveragePattern() {
        FraiseQL.registerType(TimeSeriesFactTable.class);

        FraiseQL.query("movingAverageRevenue")
            .returnType("MovingAverageData")
            .returnsArray(true)
            .arg("windowSize", "Int")
            .arg("startDate", "String")
            .arg("endDate", "String")
            .register();

        var query = registry.getQuery("movingAverageRevenue");
        assertTrue(query.isPresent());
        assertEquals(3, query.get().arguments.size());
    }

    // =========================================================================
    // COMPARATIVE ANALYTICS PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Year-over-year (YoY) comparison")
    void testYearOverYearPattern() {
        FraiseQL.registerType(TimeSeriesFactTable.class);

        FraiseQL.query("yearOverYearRevenue")
            .returnType("YoYComparison")
            .returnsArray(true)
            .arg("year1", "Int")
            .arg("year2", "Int")
            .arg("region", "String")
            .register();

        var query = registry.getQuery("yearOverYearRevenue");
        assertTrue(query.isPresent());
        assertEquals(3, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Month-over-month (MoM) growth")
    void testMonthOverMonthPattern() {
        FraiseQL.registerType(TimeSeriesFactTable.class);

        FraiseQL.query("monthOverMonthGrowth")
            .returnType("GrowthMetrics")
            .returnsArray(true)
            .arg("startMonth", "String")
            .arg("endMonth", "String")
            .register();

        var query = registry.getQuery("monthOverMonthGrowth");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // RATIO AND EFFICIENCY METRICS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Profit margin calculation (profit / revenue)")
    void testProfitMarginPattern() {
        FraiseQL.registerType(FinancialFactTable.class);

        FraiseQL.query("profitMarginAnalysis")
            .returnType("MarginAnalysis")
            .returnsArray(true)
            .arg("minMarginPercent", "Float")
            .arg("maxMarginPercent", "Float")
            .register();

        var query = registry.getQuery("profitMarginAnalysis");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Conversion funnel (steps through customer journey)")
    void testConversionFunnelPattern() {
        FraiseQL.registerType(UserJourneyFactTable.class);

        FraiseQL.query("conversionFunnel")
            .returnType("FunnelStep")
            .returnsArray(true)
            .arg("startDate", "String")
            .arg("endDate", "String")
            .register();

        var query = registry.getQuery("conversionFunnel");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // RANKING AND SEGMENTATION PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Top N products by revenue")
    void testTopNPattern() {
        FraiseQL.registerType(ProductSalesFactTable.class);

        FraiseQL.query("topProductsByRevenue")
            .returnType("RankedProduct")
            .returnsArray(true)
            .arg("topN", "Int")
            .arg("region", "String")
            .register();

        var query = registry.getQuery("topProductsByRevenue");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    @Test
    @DisplayName("Pattern: Customer segmentation (RFM analysis)")
    void testCustomerSegmentationPattern() {
        FraiseQL.registerType(CustomerSalesFactTable.class);

        FraiseQL.query("rfmSegmentation")
            .returnType("RFMSegment")
            .returnsArray(true)
            .arg("recencyDays", "Int")
            .arg("minFrequency", "Int")
            .arg("minMonetaryValue", "Float")
            .register();

        var query = registry.getQuery("rfmSegmentation");
        assertTrue(query.isPresent());
        assertEquals(3, query.get().arguments.size());
    }

    // =========================================================================
    // CUSTOMER LIFETIME VALUE (CLV) PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Customer lifetime value (CLV) calculation")
    void testCustomerLifetimeValuePattern() {
        FraiseQL.registerType(CustomerLifecycleFactTable.class);

        FraiseQL.query("customerLifetimeValue")
            .returnType("CLVMetrics")
            .returnsArray(true)
            .arg("minCLV", "Float")
            .arg("acquisitionChannel", "String")
            .register();

        var query = registry.getQuery("customerLifetimeValue");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // MARKETING METRICS PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Campaign ROI (Return on Investment) analysis")
    void testCampaignROIPattern() {
        FraiseQL.registerType(MarketingCampaignFactTable.class);

        FraiseQL.query("campaignROI")
            .returnType("ROIMetrics")
            .returnsArray(true)
            .arg("minROI", "Float")
            .arg("channel", "String")
            .register();

        var query = registry.getQuery("campaignROI");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // COHORT AND RETENTION PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Cohort retention analysis")
    void testCohortRetentionPattern() {
        FraiseQL.registerType(CohortFactTable.class);

        FraiseQL.query("cohortRetention")
            .returnType("CohortRetention")
            .returnsArray(true)
            .arg("cohortDate", "String")
            .arg("weeksToTrack", "Int")
            .register();

        var query = registry.getQuery("cohortRetention");
        assertTrue(query.isPresent());
        assertEquals(2, query.get().arguments.size());
    }

    // =========================================================================
    // MULTI-DIMENSIONAL ANALYSIS PATTERNS
    // =========================================================================

    @Test
    @DisplayName("Pattern: Cross-dimensional drill-down (region → product → customer)")
    void testCrossDimensionalDrillDownPattern() {
        FraiseQL.registerType(UniversalSalesFactTable.class);

        FraiseQL.query("drillDownAnalysis")
            .returnType("DrillDownData")
            .returnsArray(true)
            .arg("region", "String")
            .arg("product", "String")
            .arg("customer", "String")
            .arg("startDate", "String")
            .arg("endDate", "String")
            .register();

        var query = registry.getQuery("drillDownAnalysis");
        assertTrue(query.isPresent());
        assertEquals(5, query.get().arguments.size());
    }

    // =========================================================================
    // TEST FIXTURES - FACT TABLES
    // =========================================================================

    @GraphQLFactTable(tableName = "tf_transactions", grain = "transaction")
    public static class TransactionalFactTable {
        @Measure(aggregation = "SUM", description = "Total revenue")
        public float revenue;

        @Measure(aggregation = "SUM", description = "Total cost")
        public float cost;

        @Measure(aggregation = "COUNT", description = "Transaction count")
        public long transactionCount;

        @Dimension(name = "date", hierarchy = "year > quarter > month > day")
        public String date;

        @Dimension(name = "region", hierarchy = "continent > country > region")
        public String region;

        @Dimension(name = "product")
        public String productCategory;
    }

    @GraphQLFactTable(tableName = "tf_timeseries", grain = "daily")
    public static class TimeSeriesFactTable {
        @Measure(aggregation = "SUM", description = "Daily revenue")
        public float dailyRevenue;

        @Measure(aggregation = "COUNT", description = "Daily transactions")
        public long dailyTransactions;

        @Measure(aggregation = "AVG", description = "Average transaction value")
        public float avgValue;

        @Dimension(name = "date", hierarchy = "year > month > day", cardinality = 365)
        public String date;

        @Dimension(name = "region", cardinality = 4)
        public String region;
    }

    @GraphQLFactTable(tableName = "tf_financial")
    public static class FinancialFactTable {
        @Measure(aggregation = "SUM", description = "Revenue in USD", unit = "USD")
        public float revenue;

        @Measure(aggregation = "SUM", description = "Cost of goods", unit = "USD")
        public float cost;

        @Measure(aggregation = "SUM", description = "Gross profit", unit = "USD")
        public float profit;

        @Dimension(name = "date", hierarchy = "year > quarter > month")
        public String date;

        @Dimension(name = "business_unit")
        public String businessUnit;
    }

    @GraphQLFactTable(tableName = "tf_user_journey")
    public static class UserJourneyFactTable {
        @Measure(aggregation = "COUNT", description = "Users reaching step")
        public long stepCount;

        @Measure(aggregation = "AVG", description = "Average time in step (seconds)")
        public float avgDuration;

        @Dimension(name = "step", cardinality = 5)
        public String step;

        @Dimension(name = "date", hierarchy = "year > month > day")
        public String date;
    }

    @GraphQLFactTable(tableName = "tf_product_sales")
    public static class ProductSalesFactTable {
        @Measure(aggregation = "SUM", description = "Product revenue")
        public float revenue;

        @Measure(aggregation = "COUNT", description = "Units sold")
        public long unitsSold;

        @Dimension(name = "product", cardinality = 1000)
        public String productId;

        @Dimension(name = "date", hierarchy = "year > month > day")
        public String date;

        @Dimension(name = "region")
        public String region;
    }

    @GraphQLFactTable(tableName = "tf_customer_sales")
    public static class CustomerSalesFactTable {
        @Measure(aggregation = "SUM", description = "Customer total revenue")
        public float revenue;

        @Measure(aggregation = "COUNT", description = "Customer transaction count")
        public long transactionCount;

        @Dimension(name = "customer_id", cardinality = 100000)
        public String customerId;

        @Dimension(name = "last_purchase_date", hierarchy = "year > month > day")
        public String lastPurchaseDate;
    }

    @GraphQLFactTable(tableName = "tf_customer_lifecycle")
    public static class CustomerLifecycleFactTable {
        @Measure(aggregation = "SUM", description = "Lifetime revenue per customer")
        public float lifetimeRevenue;

        @Measure(aggregation = "AVG", description = "Average order value")
        public float avgOrderValue;

        @Measure(aggregation = "COUNT", description = "Total orders")
        public long totalOrders;

        @Dimension(name = "customer_id", cardinality = 100000)
        public String customerId;

        @Dimension(name = "acquisition_channel")
        public String channel;

        @Dimension(name = "first_purchase_date", hierarchy = "year > month > day")
        public String firstPurchaseDate;
    }

    @GraphQLFactTable(tableName = "tf_marketing_campaign")
    public static class MarketingCampaignFactTable {
        @Measure(aggregation = "SUM", description = "Campaign spend in USD", unit = "USD")
        public float spend;

        @Measure(aggregation = "COUNT", description = "Impressions")
        public long impressions;

        @Measure(aggregation = "COUNT", description = "Clicks")
        public long clicks;

        @Measure(aggregation = "SUM", description = "Revenue from campaign", unit = "USD")
        public float revenue;

        @Dimension(name = "campaign_id")
        public String campaignId;

        @Dimension(name = "channel")
        public String channel;

        @Dimension(name = "date", hierarchy = "year > month > day")
        public String date;
    }

    @GraphQLFactTable(tableName = "tf_cohort")
    public static class CohortFactTable {
        @Measure(aggregation = "COUNT", description = "Users in cohort")
        public long cohortSize;

        @Measure(aggregation = "AVG", description = "Retention percentage")
        public float retentionPercent;

        @Dimension(name = "cohort_date", hierarchy = "year > month", cardinality = 12)
        public String cohortDate;

        @Dimension(name = "week_number", cardinality = 52)
        public int weekNumber;
    }

    @GraphQLFactTable(tableName = "tf_universal_sales")
    public static class UniversalSalesFactTable {
        @Measure(aggregation = "SUM", description = "Sales amount")
        public float amount;

        @Measure(aggregation = "COUNT", description = "Transaction count")
        public long transactions;

        @Measure(aggregation = "AVG", description = "Average transaction")
        public float avgTransaction;

        @Dimension(name = "region", hierarchy = "continent > country > region")
        public String region;

        @Dimension(name = "product", cardinality = 1000)
        public String product;

        @Dimension(name = "customer", cardinality = 100000)
        public String customer;

        @Dimension(name = "date", hierarchy = "year > month > day")
        public String date;
    }

    // =========================================================================
    // TEST FIXTURES - RESULT TYPES
    // =========================================================================

    @GraphQLType
    public static class SalesTotals {
        @GraphQLField
        public float totalRevenue;

        @GraphQLField
        public long transactionCount;
    }

    @GraphQLType
    public static class ProfitMargin {
        @GraphQLField
        public float revenue;

        @GraphQLField
        public float margin;

        @GraphQLField
        public float marginPercent;
    }

    @GraphQLType
    public static class CumulativeData {
        @GraphQLField
        public String date;

        @GraphQLField
        public float cumulativeRevenue;

        @GraphQLField
        public float periodRevenue;
    }

    @GraphQLType
    public static class MovingAverageData {
        @GraphQLField
        public String date;

        @GraphQLField
        public float movingAverage;

        @GraphQLField
        public float actualValue;
    }

    @GraphQLType
    public static class YoYComparison {
        @GraphQLField
        public String period;

        @GraphQLField
        public float year1Revenue;

        @GraphQLField
        public float year2Revenue;

        @GraphQLField
        public float growthPercent;
    }

    @GraphQLType
    public static class GrowthMetrics {
        @GraphQLField
        public String month;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public float growth;

        @GraphQLField
        public float growthPercent;
    }

    @GraphQLType
    public static class MarginAnalysis {
        @GraphQLField
        public String region;

        @GraphQLField
        public float profitMargin;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public float profit;
    }

    @GraphQLType
    public static class FunnelStep {
        @GraphQLField
        public String step;

        @GraphQLField
        public long userCount;

        @GraphQLField
        public float conversionPercent;
    }

    @GraphQLType
    public static class RankedProduct {
        @GraphQLField
        public String product;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public int rank;
    }

    @GraphQLType
    public static class RFMSegment {
        @GraphQLField
        public String customerId;

        @GraphQLField
        public int recencyScore;

        @GraphQLField
        public int frequencyScore;

        @GraphQLField
        public int monetaryScore;

        @GraphQLField
        public String segment;
    }

    @GraphQLType
    public static class CLVMetrics {
        @GraphQLField
        public String customerId;

        @GraphQLField
        public float lifetimeValue;

        @GraphQLField
        public float avgOrderValue;

        @GraphQLField
        public int totalOrders;
    }

    @GraphQLType
    public static class ROIMetrics {
        @GraphQLField
        public String campaign;

        @GraphQLField
        public float spend;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public float roi;

        @GraphQLField
        public float roiPercent;
    }

    @GraphQLType
    public static class CohortRetention {
        @GraphQLField
        public String cohortDate;

        @GraphQLField
        public int week;

        @GraphQLField
        public long usersRemaining;

        @GraphQLField
        public float retentionPercent;
    }

    @GraphQLType
    public static class DrillDownData {
        @GraphQLField
        public String region;

        @GraphQLField
        public String product;

        @GraphQLField
        public String customer;

        @GraphQLField
        public float revenue;

        @GraphQLField
        public long transactionCount;

        @GraphQLField
        public float avgTransactionValue;
    }
}
