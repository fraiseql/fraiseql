<?php

declare(strict_types=1);

namespace FraiseQL\Exceptions;

class AuthenticationException extends FraiseQLException
{
    public function __construct(int $httpCode = 401, string $message = '')
    {
        parent::__construct($message ?: "Authentication failed (HTTP {$httpCode})", $httpCode);
    }
}
