package com.fraiseql.core;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Phase 6 tests: Caching, performance optimization, and monitoring
 */
public class Phase6OptimizationTest {

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
        SchemaCache.getInstance().clear();
        PerformanceMonitor.getInstance().reset();
    }

    /**
     * Test schema cache singleton
     */
    @Test
    public void testSchemaCacheSingleton() {
        SchemaCache cache1 = SchemaCache.getInstance();
        SchemaCache cache2 = SchemaCache.getInstance();
        assertSame(cache1, cache2);
    }

    /**
     * Test field cache put and get
     */
    @Test
    public void testFieldCachePutGet() {
        var fields = TypeConverter.extractFields(TestType.class);
        SchemaCache cache = SchemaCache.getInstance();

        cache.putFieldCache(TestType.class, fields);
        var cached = cache.getFieldCache(TestType.class);

        assertNotNull(cached);
        assertEquals(fields.size(), cached.size());
    }

    /**
     * Test type conversion cache
     */
    @Test
    public void testTypeConversionCache() {
        SchemaCache cache = SchemaCache.getInstance();

        cache.putTypeConversion(String.class, "String");
        cache.putTypeConversion(Integer.class, "Int");

        assertEquals("String", cache.getTypeConversion(String.class));
        assertEquals("Int", cache.getTypeConversion(Integer.class));
        assertNull(cache.getTypeConversion(Long.class));
    }

    /**
     * Test type validation cache
     */
    @Test
    public void testTypeValidationCache() {
        SchemaCache cache = SchemaCache.getInstance();

        cache.putTypeValidation("User", true);
        cache.putTypeValidation("InvalidType", false);

        assertTrue(cache.getTypeValidation("User"));
        assertFalse(cache.getTypeValidation("InvalidType"));
        assertNull(cache.getTypeValidation("NotCached"));
    }

    /**
     * Test cache stats tracking
     */
    @Test
    public void testCacheStats() {
        SchemaCache cache = SchemaCache.getInstance();
        var fields = TypeConverter.extractFields(TestType.class);

        // Record some cache hits
        cache.putTypeConversion(String.class, "String");
        cache.getTypeConversion(String.class);  // Hit

        cache.putTypeValidation("User", true);
        cache.getTypeValidation("User");  // Hit
        cache.getTypeValidation("User");  // Hit

        var stats = cache.getStats();
        assertEquals(1, stats.getTypeConversionHits());
        assertEquals(2, stats.getValidationHits());
        assertEquals(3, stats.getTotalHits());
    }

    /**
     * Test cache size info
     */
    @Test
    public void testCacheSizeInfo() {
        SchemaCache cache = SchemaCache.getInstance();

        cache.putTypeConversion(String.class, "String");
        cache.putTypeConversion(Integer.class, "Int");
        cache.putTypeValidation("User", true);

        var sizeInfo = cache.getSizeInfo();
        assertEquals(2, sizeInfo.typeConversionCacheSize);
        assertEquals(1, sizeInfo.validationCacheSize);
        assertEquals(3, sizeInfo.getTotalEntries());
    }

    /**
     * Test cache clear
     */
    @Test
    public void testCacheClear() {
        SchemaCache cache = SchemaCache.getInstance();

        cache.putTypeConversion(String.class, "String");
        cache.putTypeValidation("User", true);

        assertNotNull(cache.getTypeConversion(String.class));
        assertTrue(cache.getTypeValidation("User"));

        cache.clear();

        assertNull(cache.getTypeConversion(String.class));
        assertNull(cache.getTypeValidation("User"));
    }

    /**
     * Test performance monitor singleton
     */
    @Test
    public void testPerformanceMonitorSingleton() {
        PerformanceMonitor mon1 = PerformanceMonitor.getInstance();
        PerformanceMonitor mon2 = PerformanceMonitor.getInstance();
        assertSame(mon1, mon2);
    }

    /**
     * Test record operation
     */
    @Test
    public void testRecordOperation() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        monitor.recordOperation("typeConversion", 10);
        monitor.recordOperation("typeConversion", 20);
        monitor.recordOperation("typeConversion", 30);

        var metrics = monitor.getMetrics("typeConversion");
        assertNotNull(metrics);
        assertEquals(3, metrics.getOperationCount());
        assertEquals(20.0, metrics.getAverageLatency());
    }

    /**
     * Test operation metrics latency bounds
     */
    @Test
    public void testOperationMetricsLatency() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        monitor.recordOperation("operation", 5);
        monitor.recordOperation("operation", 15);
        monitor.recordOperation("operation", 10);

        var metrics = monitor.getMetrics("operation");
        assertEquals(5, metrics.getMinLatency());
        assertEquals(15, metrics.getMaxLatency());
        assertEquals(10.0, metrics.getAverageLatency());
    }

    /**
     * Test system metrics calculation
     */
    @Test
    public void testSystemMetrics() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        monitor.recordOperation("op1", 10);
        monitor.recordOperation("op1", 20);
        monitor.recordOperation("op2", 15);

        var sysMet = monitor.getSystemMetrics();
        assertEquals(3, sysMet.getTotalOperations());
        assertEquals(2, sysMet.getTrackedOperations());
        assertTrue(sysMet.getAverageLatency() > 0);
    }

    /**
     * Test throughput calculation
     */
    @Test
    public void testThroughput() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        for (int i = 0; i < 100; i++) {
            monitor.recordOperation("test", 1);
        }

        var sysMet = monitor.getSystemMetrics();
        assertEquals(100, sysMet.getTotalOperations());
        assertTrue(sysMet.getOperationsPerSecond() > 0);
    }

    /**
     * Test performance report generation
     */
    @Test
    public void testPerformanceReport() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        monitor.recordOperation("typeConversion", 10);
        monitor.recordOperation("typeConversion", 20);
        monitor.recordOperation("fieldExtraction", 5);

        String report = monitor.generateReport();
        assertTrue(report.contains("Performance Report"));
        assertTrue(report.contains("typeConversion"));
        assertTrue(report.contains("fieldExtraction"));
        assertTrue(report.contains("Average Latency"));
    }

    /**
     * Test operation metrics string representation
     */
    @Test
    public void testOperationMetricsToString() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();
        monitor.recordOperation("test", 10);

        var metrics = monitor.getMetrics("test");
        String str = metrics.toString();
        assertTrue(str.contains("test"));
        assertTrue(str.contains("count=1"));
    }

    /**
     * Test system metrics string representation
     */
    @Test
    public void testSystemMetricsToString() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();
        monitor.recordOperation("test", 10);

        var sysMet = monitor.getSystemMetrics();
        String str = sysMet.toString();
        assertTrue(str.contains("SystemMetrics"));
        assertTrue(str.contains("totalOps"));
    }

    /**
     * Test performance monitor reset
     */
    @Test
    public void testMonitorReset() {
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        monitor.recordOperation("test", 10);
        assertNotNull(monitor.getMetrics("test"));

        monitor.reset();

        assertNull(monitor.getMetrics("test"));
        assertEquals(0, monitor.getAllMetrics().size());
    }

    /**
     * Test cache efficiency with repeated lookups
     */
    @Test
    public void testCacheEfficiency() {
        SchemaCache cache = SchemaCache.getInstance();

        // Simulate repeated type conversions
        for (int i = 0; i < 100; i++) {
            cache.putTypeConversion(String.class, "String");
            cache.getTypeConversion(String.class);
        }

        var stats = cache.getStats();
        assertEquals(100, stats.getTypeConversionHits());
    }

    /**
     * Test combined caching and monitoring
     */
    @Test
    public void testCachingAndMonitoring() {
        SchemaCache cache = SchemaCache.getInstance();
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        long startTime = System.currentTimeMillis();

        // Simulate schema operations with caching
        for (int i = 0; i < 50; i++) {
            cache.putTypeConversion(String.class, "String");
            cache.getTypeConversion(String.class);
        }

        long duration = System.currentTimeMillis() - startTime;
        monitor.recordOperation("caching", duration);

        var metrics = monitor.getMetrics("caching");
        assertNotNull(metrics);
        assertEquals(1, metrics.getOperationCount());

        var stats = cache.getStats();
        assertEquals(50, stats.getTypeConversionHits());
    }

    /**
     * Test cache with schema operations
     */
    @Test
    public void testCacheWithSchema() {
        FraiseQL.registerType(TestType.class);

        SchemaCache cache = SchemaCache.getInstance();
        var fields = TypeConverter.extractFields(TestType.class);

        cache.putFieldCache(TestType.class, fields);

        var cached = cache.getFieldCache(TestType.class);
        assertNotNull(cached);
        assertEquals(fields.size(), cached.size());
    }

    // Test fixture
    @GraphQLType
    public static class TestType {
        @GraphQLField
        public int id;

        @GraphQLField
        public String name;
    }
}
