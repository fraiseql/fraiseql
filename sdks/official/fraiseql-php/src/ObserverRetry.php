<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * How an observer retries an action that fails (#2147).
 *
 * Not {@see RetryConfig}, which is the client's retry of a request. An observer given
 * none runs on the server's own policy.
 */
final class ObserverRetry
{
    /**
     * @param string $backoffStrategy `fixed`, `linear` or `exponential`.
     */
    public function __construct(
        public readonly int $maxAttempts,
        public readonly string $backoffStrategy,
        public readonly int $initialDelayMs,
        public readonly int $maxDelayMs,
    ) {
    }

    /** @return array{max_attempts: int, backoff_strategy: string, initial_delay_ms: int, max_delay_ms: int} */
    public function toArray(): array
    {
        return [
            'max_attempts' => $this->maxAttempts,
            'backoff_strategy' => $this->backoffStrategy,
            'initial_delay_ms' => $this->initialDelayMs,
            'max_delay_ms' => $this->maxDelayMs,
        ];
    }
}
