<?php

declare(strict_types=1);

namespace FraiseQL\Exceptions;

class RateLimitException extends FraiseQLException
{
    public function __construct(string $message = 'Rate limit exceeded (HTTP 429)')
    {
        parent::__construct($message, 429);
    }
}
