package io.fraiseql;

import java.time.Duration;

/** Configuration for HTTP request retry behaviour in {@link FraiseQLClient}. */
public final class RetryConfig {
    private final int maxAttempts;
    private final Duration baseDelay;
    private final Duration maxDelay;
    private final boolean jitter;

    private RetryConfig(Builder builder) {
        this.maxAttempts = builder.maxAttempts;
        this.baseDelay = builder.baseDelay;
        this.maxDelay = builder.maxDelay;
        this.jitter = builder.jitter;
    }

    /** Returns a {@link RetryConfig} that performs no retries (single attempt). */
    public static RetryConfig noRetry() {
        return builder().maxAttempts(1).build();
    }

    public static Builder builder() { return new Builder(); }

    public int getMaxAttempts() { return maxAttempts; }

    public Duration getBaseDelay() { return baseDelay; }

    public Duration getMaxDelay() { return maxDelay; }

    public boolean isJitter() { return jitter; }

    public static final class Builder {
        private int maxAttempts = 1;
        private Duration baseDelay = Duration.ofSeconds(1);
        private Duration maxDelay = Duration.ofSeconds(30);
        private boolean jitter = true;

        public Builder maxAttempts(int n) { this.maxAttempts = n; return this; }

        public Builder baseDelay(Duration d) { this.baseDelay = d; return this; }

        public Builder maxDelay(Duration d) { this.maxDelay = d; return this; }

        public Builder jitter(boolean j) { this.jitter = j; return this; }

        public RetryConfig build() { return new RetryConfig(this); }
    }
}
