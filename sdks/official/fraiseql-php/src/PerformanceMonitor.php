<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Monitors and tracks performance metrics for FraiseQL operations.
 *
 * Provides comprehensive performance tracking for:
 * - Operation timing (registry operations, formatting, validation)
 * - Memory usage tracking
 * - Operation counts
 * - Performance statistics and analysis
 */
final class PerformanceMonitor
{
    /** @var array<string, array> Operation metrics indexed by operation name */
    private array $metrics = [];

    /** @var array<string, float> Active operation start times */
    private array $activeOperations = [];

    /** @var bool Whether monitoring is enabled */
    private bool $enabled = true;

    /**
     * Start timing an operation.
     *
     * @param string $operationName Name of operation to time
     * @return void
     */
    public function startOperation(string $operationName): void
    {
        if (!$this->enabled) {
            return;
        }

        $this->activeOperations[$operationName] = microtime(true);
    }

    /**
     * End timing an operation and record metrics.
     *
     * @param string $operationName Name of operation to stop
     * @return float Time in seconds, 0 if operation not started
     */
    public function endOperation(string $operationName): float
    {
        if (!$this->enabled || !isset($this->activeOperations[$operationName])) {
            return 0.0;
        }

        $startTime = $this->activeOperations[$operationName];
        $duration = microtime(true) - $startTime;

        unset($this->activeOperations[$operationName]);

        // Record metric
        if (!isset($this->metrics[$operationName])) {
            $this->metrics[$operationName] = [
                'count' => 0,
                'total_time' => 0.0,
                'min_time' => PHP_FLOAT_MAX,
                'max_time' => 0.0,
                'last_time' => 0.0,
            ];
        }

        $this->metrics[$operationName]['count']++;
        $this->metrics[$operationName]['total_time'] += $duration;
        $this->metrics[$operationName]['min_time'] = min($this->metrics[$operationName]['min_time'], $duration);
        $this->metrics[$operationName]['max_time'] = max($this->metrics[$operationName]['max_time'], $duration);
        $this->metrics[$operationName]['last_time'] = $duration;

        return $duration;
    }

    /**
     * Get metrics for a specific operation.
     *
     * @param string $operationName Operation name
     * @return array<string, mixed>|null Operation metrics or null if not recorded
     */
    public function getOperationMetrics(string $operationName): ?array
    {
        if (!isset($this->metrics[$operationName])) {
            return null;
        }

        $metric = $this->metrics[$operationName];

        return [
            'name' => $operationName,
            'count' => $metric['count'],
            'total_time' => $metric['total_time'],
            'average_time' => $metric['count'] > 0 ? $metric['total_time'] / $metric['count'] : 0,
            'min_time' => $metric['min_time'] === PHP_FLOAT_MAX ? 0 : $metric['min_time'],
            'max_time' => $metric['max_time'],
            'last_time' => $metric['last_time'],
        ];
    }

    /**
     * Get all recorded metrics.
     *
     * @return array<string, array>
     */
    public function getAllMetrics(): array
    {
        $results = [];

        foreach (array_keys($this->metrics) as $operationName) {
            $results[$operationName] = $this->getOperationMetrics($operationName);
        }

        return $results;
    }

    /**
     * Get operation names that have been recorded.
     *
     * @return string[]
     */
    public function getOperationNames(): array
    {
        return array_keys($this->metrics);
    }

    /**
     * Get total time spent in all operations.
     *
     * @return float Total time in seconds
     */
    public function getTotalTime(): float
    {
        return array_sum(array_map(
            static fn(array $m) => $m['total_time'],
            $this->metrics
        ));
    }

    /**
     * Get total operation count.
     *
     * @return int Number of operations recorded
     */
    public function getTotalOperationCount(): int
    {
        return array_sum(array_map(
            static fn(array $m) => $m['count'],
            $this->metrics
        ));
    }

    /**
     * Get average time per operation across all operations.
     *
     * @return float Average time in seconds
     */
    public function getAverageTime(): float
    {
        $total = $this->getTotalOperationCount();
        return $total > 0 ? $this->getTotalTime() / $total : 0.0;
    }

    /**
     * Get slowest operation.
     *
     * @return array<string, mixed>|null Slowest operation metrics or null if no data
     */
    public function getSlowestOperation(): ?array
    {
        if (empty($this->metrics)) {
            return null;
        }

        $slowest = null;
        $maxTime = 0;

        foreach ($this->metrics as $name => $metric) {
            if ($metric['total_time'] > $maxTime) {
                $maxTime = $metric['total_time'];
                $slowest = $this->getOperationMetrics($name);
            }
        }

        return $slowest;
    }

    /**
     * Get fastest operation.
     *
     * @return array<string, mixed>|null Fastest operation metrics or null if no data
     */
    public function getFastestOperation(): ?array
    {
        if (empty($this->metrics)) {
            return null;
        }

        $fastest = null;
        $minTime = PHP_FLOAT_MAX;

        foreach ($this->metrics as $name => $metric) {
            if ($metric['total_time'] < $minTime) {
                $minTime = $metric['total_time'];
                $fastest = $this->getOperationMetrics($name);
            }
        }

        return $fastest;
    }

    /**
     * Get top N slowest operations.
     *
     * @param int $count Number of operations to return
     * @return array<string, array>
     */
    public function getTopSlowOperations(int $count = 5): array
    {
        $all = $this->getAllMetrics();

        usort($all, static fn(array $a, array $b) => $b['total_time'] <=> $a['total_time']);

        return array_slice($all, 0, $count);
    }

    /**
     * Clear all recorded metrics.
     *
     * @return void
     */
    public function clear(): void
    {
        $this->metrics = [];
        $this->activeOperations = [];
    }

    /**
     * Enable performance monitoring.
     *
     * @return void
     */
    public function enable(): void
    {
        $this->enabled = true;
    }

    /**
     * Disable performance monitoring.
     *
     * @return void
     */
    public function disable(): void
    {
        $this->enabled = false;
    }

    /**
     * Check if monitoring is enabled.
     *
     * @return bool
     */
    public function isEnabled(): bool
    {
        return $this->enabled;
    }

    /**
     * Get formatted performance report.
     *
     * @return string Formatted report
     */
    public function getReport(): string
    {
        if (empty($this->metrics)) {
            return 'No performance metrics recorded';
        }

        $report = [];
        $report[] = '=== Performance Report ===';
        $report[] = sprintf('Total Time: %.4f seconds', $this->getTotalTime());
        $report[] = sprintf('Total Operations: %d', $this->getTotalOperationCount());
        $report[] = sprintf('Average Time per Operation: %.6f seconds', $this->getAverageTime());
        $report[] = '';
        $report[] = '=== Operation Breakdown ===';

        foreach ($this->getAllMetrics() as $metrics) {
            $report[] = sprintf(
                '%s: %d calls, %.4f total, %.6f avg, %.6f min, %.6f max',
                $metrics['name'],
                $metrics['count'],
                $metrics['total_time'],
                $metrics['average_time'],
                $metrics['min_time'],
                $metrics['max_time']
            );
        }

        $slowest = $this->getSlowestOperation();
        if ($slowest !== null) {
            $report[] = '';
            $report[] = sprintf('Slowest: %s (%.4f seconds)', $slowest['name'], $slowest['total_time']);
        }

        return implode("\n", $report);
    }

    /**
     * Get summary statistics.
     *
     * @return array<string, mixed>
     */
    public function getSummary(): array
    {
        return [
            'total_time' => $this->getTotalTime(),
            'total_operations' => $this->getTotalOperationCount(),
            'average_time' => $this->getAverageTime(),
            'operations_count' => count($this->metrics),
            'slowest' => $this->getSlowestOperation(),
            'fastest' => $this->getFastestOperation(),
        ];
    }
}
