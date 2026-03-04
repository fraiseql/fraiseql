package com.fraiseql.core;

/**
 * Configuration for observer action retry behaviour.
 */
public final class RetryConfig {

    private final int maxAttempts;
    private final String backoffStrategy;
    private final int initialDelayMs;
    private final int maxDelayMs;

    private RetryConfig(int maxAttempts, String backoffStrategy, int initialDelayMs, int maxDelayMs) {
        this.maxAttempts = maxAttempts;
        this.backoffStrategy = backoffStrategy;
        this.initialDelayMs = initialDelayMs;
        this.maxDelayMs = maxDelayMs;
    }

    /**
     * Default retry config: 3 attempts, exponential back-off starting at 100 ms, capped at 60 s.
     */
    public static RetryConfig defaults() {
        return new RetryConfig(3, "exponential", 100, 60_000);
    }

    /**
     * Exponential back-off retry configuration.
     *
     * @param maxAttempts    maximum number of delivery attempts
     * @param initialDelayMs delay before the first retry (milliseconds)
     * @param maxDelayMs     upper bound for back-off delay (milliseconds)
     */
    public static RetryConfig exponential(int maxAttempts, int initialDelayMs, int maxDelayMs) {
        return new RetryConfig(maxAttempts, "exponential", initialDelayMs, maxDelayMs);
    }

    /**
     * Fixed-interval retry configuration.
     *
     * @param maxAttempts maximum number of delivery attempts
     * @param intervalMs  fixed delay between retries (milliseconds)
     */
    public static RetryConfig fixed(int maxAttempts, int intervalMs) {
        return new RetryConfig(maxAttempts, "fixed", intervalMs, intervalMs);
    }

    public int getMaxAttempts() {
        return maxAttempts;
    }

    public String getBackoffStrategy() {
        return backoffStrategy;
    }

    public int getInitialDelayMs() {
        return initialDelayMs;
    }

    public int getMaxDelayMs() {
        return maxDelayMs;
    }

    @Override
    public String toString() {
        return "RetryConfig{maxAttempts=" + maxAttempts
            + ", backoffStrategy='" + backoffStrategy + '\''
            + ", initialDelayMs=" + initialDelayMs
            + ", maxDelayMs=" + maxDelayMs + '}';
    }
}
