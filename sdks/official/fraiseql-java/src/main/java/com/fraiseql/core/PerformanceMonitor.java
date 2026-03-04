package com.fraiseql.core;

import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Singleton performance monitor for tracking FraiseQL schema operation latencies.
 */
public final class PerformanceMonitor {

    private static final PerformanceMonitor INSTANCE = new PerformanceMonitor();

    private final Map<String, OperationMetrics> metricsMap = new ConcurrentHashMap<>();
    private final AtomicLong startTimeMs = new AtomicLong(System.currentTimeMillis());

    private PerformanceMonitor() {
    }

    public static PerformanceMonitor getInstance() {
        return INSTANCE;
    }

    /**
     * Record a single operation with its latency.
     *
     * @param operationName name of the operation
     * @param latencyMs     latency in milliseconds
     */
    public void recordOperation(String operationName, long latencyMs) {
        metricsMap.computeIfAbsent(operationName, OperationMetrics::new)
                  .record(latencyMs);
    }

    /**
     * Get metrics for a named operation.
     *
     * @param operationName name of the operation
     * @return OperationMetrics or null if not recorded
     */
    public OperationMetrics getMetrics(String operationName) {
        return metricsMap.get(operationName);
    }

    /**
     * Get an unmodifiable view of all tracked metrics.
     */
    public Map<String, OperationMetrics> getAllMetrics() {
        return Collections.unmodifiableMap(metricsMap);
    }

    /**
     * Compute aggregate system-level metrics across all operations.
     */
    public SystemMetrics getSystemMetrics() {
        long total = 0;
        long sumLatency = 0;
        for (OperationMetrics m : metricsMap.values()) {
            total += m.getOperationCount();
            sumLatency += m.getTotalLatency();
        }
        long elapsedMs = Math.max(1, System.currentTimeMillis() - startTimeMs.get());
        double opsPerSec = total * 1000.0 / elapsedMs;
        double avgLatency = total > 0 ? (double) sumLatency / total : 0.0;
        return new SystemMetrics(total, metricsMap.size(), avgLatency, opsPerSec);
    }

    /**
     * Generate a human-readable performance report.
     */
    public String generateReport() {
        StringBuilder sb = new StringBuilder("Performance Report\n");
        sb.append("==================\n");
        for (Map.Entry<String, OperationMetrics> e : metricsMap.entrySet()) {
            OperationMetrics m = e.getValue();
            sb.append(String.format("  %s: count=%d, Average Latency=%.1f ms, min=%d ms, max=%d ms%n",
                e.getKey(), m.getOperationCount(), m.getAverageLatency(),
                m.getMinLatency(), m.getMaxLatency()));
        }
        return sb.toString();
    }

    /**
     * Reset all recorded metrics.
     */
    public void reset() {
        metricsMap.clear();
        startTimeMs.set(System.currentTimeMillis());
    }

    /**
     * Metrics for a single named operation.
     */
    public static final class OperationMetrics {
        private final String name;
        private long count = 0;
        private long totalLatency = 0;
        private long minLatency = Long.MAX_VALUE;
        private long maxLatency = Long.MIN_VALUE;

        OperationMetrics(String name) {
            this.name = name;
        }

        synchronized void record(long latencyMs) {
            count++;
            totalLatency += latencyMs;
            if (latencyMs < minLatency) minLatency = latencyMs;
            if (latencyMs > maxLatency) maxLatency = latencyMs;
        }

        public synchronized long getOperationCount() { return count; }
        public synchronized long getTotalLatency() { return totalLatency; }
        public synchronized double getAverageLatency() { return count > 0 ? (double) totalLatency / count : 0.0; }
        public synchronized long getMinLatency() { return count > 0 ? minLatency : 0; }
        public synchronized long getMaxLatency() { return count > 0 ? maxLatency : 0; }

        @Override
        public String toString() {
            return String.format("OperationMetrics{name='%s', count=%d, avg=%.1f ms}",
                name, count, getAverageLatency());
        }
    }

    /**
     * Aggregate system-level metrics.
     */
    public static final class SystemMetrics {
        private final long totalOperations;
        private final int trackedOperations;
        private final double averageLatency;
        private final double operationsPerSecond;

        SystemMetrics(long totalOperations, int trackedOperations,
                      double averageLatency, double operationsPerSecond) {
            this.totalOperations = totalOperations;
            this.trackedOperations = trackedOperations;
            this.averageLatency = averageLatency;
            this.operationsPerSecond = operationsPerSecond;
        }

        public long getTotalOperations() { return totalOperations; }
        public int getTrackedOperations() { return trackedOperations; }
        public double getAverageLatency() { return averageLatency; }
        public double getOperationsPerSecond() { return operationsPerSecond; }

        @Override
        public String toString() {
            return String.format("SystemMetrics{totalOps=%d, tracked=%d, avg=%.1f ms, ops/s=%.1f}",
                totalOperations, trackedOperations, averageLatency, operationsPerSecond);
        }
    }
}
