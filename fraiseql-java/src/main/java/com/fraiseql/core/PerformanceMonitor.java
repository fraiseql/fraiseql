package com.fraiseql.core;

import java.util.*;
import java.util.concurrent.*;

/**
 * Performance monitoring and metrics collection for schema operations.
 * Tracks timing, operation counts, and cache efficiency.
 */
public class PerformanceMonitor {
    private static final PerformanceMonitor INSTANCE = new PerformanceMonitor();

    private final Map<String, OperationMetrics> metrics = new ConcurrentHashMap<>();
    private volatile long startTime = System.currentTimeMillis();

    private PerformanceMonitor() {
    }

    /**
     * Get the singleton monitor instance.
     *
     * @return the PerformanceMonitor instance
     */
    public static PerformanceMonitor getInstance() {
        return INSTANCE;
    }

    /**
     * Record an operation's execution time.
     *
     * @param operationName the operation name
     * @param durationMillis the duration in milliseconds
     */
    public void recordOperation(String operationName, long durationMillis) {
        metrics.computeIfAbsent(operationName, k -> new OperationMetrics(operationName))
            .recordOperation(durationMillis);
    }

    /**
     * Get metrics for a specific operation.
     *
     * @param operationName the operation name
     * @return OperationMetrics or null if not tracked
     */
    public OperationMetrics getMetrics(String operationName) {
        return metrics.get(operationName);
    }

    /**
     * Get all tracked metrics.
     *
     * @return map of operation name to metrics
     */
    public Map<String, OperationMetrics> getAllMetrics() {
        return new LinkedHashMap<>(metrics);
    }

    /**
     * Get overall system metrics.
     *
     * @return SystemMetrics with aggregate data
     */
    public SystemMetrics getSystemMetrics() {
        long uptime = System.currentTimeMillis() - startTime;
        long totalOps = metrics.values().stream()
            .mapToLong(m -> m.getOperationCount())
            .sum();
        double avgLatency = metrics.values().stream()
            .mapToDouble(m -> m.getAverageLatency())
            .average()
            .orElse(0.0);

        return new SystemMetrics(uptime, totalOps, avgLatency, metrics.size());
    }

    /**
     * Reset all metrics.
     */
    public void reset() {
        metrics.clear();
        startTime = System.currentTimeMillis();
    }

    /**
     * Get a human-readable performance report.
     *
     * @return formatted report string
     */
    public String generateReport() {
        StringBuilder sb = new StringBuilder();
        sb.append("=== Performance Report ===\n");

        SystemMetrics sys = getSystemMetrics();
        sb.append(String.format("Uptime: %d ms\n", sys.getUptimeMillis()));
        sb.append(String.format("Total Operations: %d\n", sys.getTotalOperations()));
        sb.append(String.format("Average Latency: %.2f ms\n", sys.getAverageLatency()));
        sb.append(String.format("Tracked Operations: %d\n\n", sys.getTrackedOperations()));

        sb.append("Operation Details:\n");
        for (OperationMetrics m : metrics.values()) {
            sb.append(String.format("  %s:\n", m.getName()));
            sb.append(String.format("    Count: %d\n", m.getOperationCount()));
            sb.append(String.format("    Min: %.2f ms\n", m.getMinLatency()));
            sb.append(String.format("    Max: %.2f ms\n", m.getMaxLatency()));
            sb.append(String.format("    Avg: %.2f ms\n", m.getAverageLatency()));
        }

        return sb.toString();
    }

    /**
     * Metrics for a specific operation.
     */
    public static class OperationMetrics {
        private final String name;
        private volatile long operationCount = 0;
        private volatile long totalDuration = 0;
        private volatile long minLatency = Long.MAX_VALUE;
        private volatile long maxLatency = 0;

        public OperationMetrics(String name) {
            this.name = name;
        }

        void recordOperation(long durationMillis) {
            operationCount++;
            totalDuration += durationMillis;
            minLatency = Math.min(minLatency, durationMillis);
            maxLatency = Math.max(maxLatency, durationMillis);
        }

        public String getName() {
            return name;
        }

        public long getOperationCount() {
            return operationCount;
        }

        public double getAverageLatency() {
            if (operationCount == 0) return 0.0;
            return (double) totalDuration / operationCount;
        }

        public long getMinLatency() {
            return minLatency == Long.MAX_VALUE ? 0 : minLatency;
        }

        public long getMaxLatency() {
            return maxLatency;
        }

        public long getTotalDuration() {
            return totalDuration;
        }

        @Override
        public String toString() {
            return String.format(
                "OperationMetrics{name='%s', count=%d, avg=%.2f ms, min=%d ms, max=%d ms}",
                name, operationCount, getAverageLatency(), getMinLatency(), getMaxLatency()
            );
        }
    }

    /**
     * System-wide metrics.
     */
    public static class SystemMetrics {
        private final long uptimeMillis;
        private final long totalOperations;
        private final double averageLatency;
        private final int trackedOperations;

        public SystemMetrics(long uptime, long totalOps, double avgLatency, int tracked) {
            this.uptimeMillis = uptime;
            this.totalOperations = totalOps;
            this.averageLatency = avgLatency;
            this.trackedOperations = tracked;
        }

        public long getUptimeMillis() {
            return uptimeMillis;
        }

        public long getTotalOperations() {
            return totalOperations;
        }

        public double getAverageLatency() {
            return averageLatency;
        }

        public int getTrackedOperations() {
            return trackedOperations;
        }

        /**
         * Get operations per second.
         *
         * @return throughput in ops/sec
         */
        public double getOperationsPerSecond() {
            if (uptimeMillis == 0) return 0.0;
            return (double) totalOperations * 1000 / uptimeMillis;
        }

        @Override
        public String toString() {
            return String.format(
                "SystemMetrics{uptime=%d ms, totalOps=%d, avgLatency=%.2f ms, tracked=%d, ops/sec=%.2f}",
                uptimeMillis, totalOperations, averageLatency, trackedOperations, getOperationsPerSecond()
            );
        }
    }
}
