<?php

declare(strict_types=1);

namespace FraiseQL;

use FraiseQL\Exceptions\NetworkException;
use FraiseQL\Exceptions\TimeoutException;

final class RetryConfig
{
    /** @param list<class-string<\FraiseQL\Exceptions\FraiseQLException>> $retryOn */
    public function __construct(
        public readonly int $maxAttempts = 1,
        public readonly float $baseDelaySeconds = 1.0,
        public readonly float $maxDelaySeconds = 30.0,
        public readonly bool $jitter = true,
        public readonly array $retryOn = [NetworkException::class, TimeoutException::class],
    ) {}
}
