package io.fraiseql;

import java.time.Duration;
import java.util.Optional;

/** Thrown when the server responds with HTTP 429 (Too Many Requests). */
public class RateLimitException extends FraiseQLException {
    private final Optional<Duration> retryAfter;

    public RateLimitException() { this(Optional.empty()); }

    public RateLimitException(Optional<Duration> retryAfter) {
        super("Rate limit exceeded");
        this.retryAfter = retryAfter;
    }

    public Optional<Duration> getRetryAfter() { return retryAfter; }
}
