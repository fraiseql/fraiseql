<?php

declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use FraiseQL\Exceptions\FraiseQLException;
use FraiseQL\Exceptions\GraphQLException;
use FraiseQL\Exceptions\NetworkException;
use FraiseQL\Exceptions\TimeoutException;
use FraiseQL\Exceptions\AuthenticationException;
use FraiseQL\Exceptions\RateLimitException;

class ExceptionsTest extends TestCase
{
    public function testFraiseQLExceptionIsRuntimeException(): void
    {
        $ex = new FraiseQLException('base error');
        $this->assertInstanceOf(\RuntimeException::class, $ex);
        $this->assertSame('base error', $ex->getMessage());
    }

    public function testNetworkExceptionExtendsFraiseQLException(): void
    {
        $ex = new NetworkException('connection refused');
        $this->assertInstanceOf(FraiseQLException::class, $ex);
        $this->assertSame('connection refused', $ex->getMessage());
    }

    public function testTimeoutExceptionExtendsFraiseQLException(): void
    {
        $ex = new TimeoutException('timed out');
        $this->assertInstanceOf(FraiseQLException::class, $ex);
        $this->assertSame('timed out', $ex->getMessage());
    }

    public function testAuthenticationExceptionDefaultMessage(): void
    {
        $ex = new AuthenticationException(401);
        $this->assertInstanceOf(FraiseQLException::class, $ex);
        $this->assertStringContainsString('401', $ex->getMessage());
        $this->assertSame(401, $ex->getCode());
    }

    public function testAuthenticationExceptionCustomMessage(): void
    {
        $ex = new AuthenticationException(403, 'Forbidden');
        $this->assertSame('Forbidden', $ex->getMessage());
        $this->assertSame(403, $ex->getCode());
    }

    public function testRateLimitExceptionDefaultMessage(): void
    {
        $ex = new RateLimitException();
        $this->assertInstanceOf(FraiseQLException::class, $ex);
        $this->assertStringContainsString('429', $ex->getMessage());
        $this->assertSame(429, $ex->getCode());
    }

    public function testRateLimitExceptionCustomMessage(): void
    {
        $ex = new RateLimitException('Slow down');
        $this->assertSame('Slow down', $ex->getMessage());
    }

    public function testGraphQLExceptionStoresErrors(): void
    {
        $errors = [['message' => 'Field not found'], ['message' => 'Type mismatch']];
        $ex = new GraphQLException($errors);
        $this->assertInstanceOf(FraiseQLException::class, $ex);
        $this->assertSame($errors, $ex->errors);
        $this->assertSame('Field not found', $ex->getMessage());
    }

    public function testGraphQLExceptionCustomMessage(): void
    {
        $errors = [['message' => 'Field not found']];
        $ex = new GraphQLException($errors, 'Custom message');
        $this->assertSame('Custom message', $ex->getMessage());
    }

    public function testGraphQLExceptionEmptyErrors(): void
    {
        $ex = new GraphQLException([]);
        $this->assertSame('GraphQL error', $ex->getMessage());
        $this->assertSame([], $ex->errors);
    }
}
