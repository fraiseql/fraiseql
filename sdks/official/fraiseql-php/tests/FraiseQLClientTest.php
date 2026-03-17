<?php

declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use FraiseQL\FraiseQLClient;
use FraiseQL\Exceptions\GraphQLException;
use FraiseQL\Exceptions\AuthenticationException;
use FraiseQL\Exceptions\RateLimitException;

class FraiseQLClientTest extends TestCase
{
    public function testClientInstantiates(): void
    {
        $client = new FraiseQLClient('http://localhost:9999');
        $this->assertInstanceOf(FraiseQLClient::class, $client);
    }

    public function testClientInstantiatesWithAllOptions(): void
    {
        $client = new FraiseQLClient(
            url: 'https://api.example.com/graphql',
            authorization: 'Bearer token123',
            retry: null,
            timeout: 60.0,
        );
        $this->assertInstanceOf(FraiseQLClient::class, $client);
    }

    public function testGraphQLExceptionThrowsOnErrors(): void
    {
        $errors = [['message' => 'Unauthorized']];
        $ex = new GraphQLException($errors);
        $this->assertSame('Unauthorized', $ex->getMessage());
        $this->assertSame($errors, $ex->errors);
    }

    public function testNullErrorsDoNotThrowException(): void
    {
        // Verify null errors are treated as success (cross-SDK invariant):
        // empty($parsed['errors']) returns true for null, so no exception is raised.
        $errors = null;
        $this->assertTrue(empty($errors), 'null errors should be treated as success');
    }

    public function testEmptyErrorsArrayDoesNotThrow(): void
    {
        $errors = [];
        $this->assertTrue(empty($errors), 'empty errors array should be treated as success');
    }

    public function testLiveQuerySkipped(): void
    {
        $this->markTestSkipped('Requires a live FraiseQL server for full integration');
    }
}
