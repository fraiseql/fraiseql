<?php

declare(strict_types=1);

use PHPUnit\Framework\TestCase;
use FraiseQL\RetryConfig;
use FraiseQL\Exceptions\NetworkException;
use FraiseQL\Exceptions\TimeoutException;

class RetryConfigTest extends TestCase
{
    public function testDefaultValues(): void
    {
        $config = new RetryConfig();
        $this->assertSame(1, $config->maxAttempts);
        $this->assertSame(1.0, $config->baseDelaySeconds);
        $this->assertSame(30.0, $config->maxDelaySeconds);
        $this->assertTrue($config->jitter);
        $this->assertSame([NetworkException::class, TimeoutException::class], $config->retryOn);
    }

    public function testCustomValues(): void
    {
        $config = new RetryConfig(
            maxAttempts: 3,
            baseDelaySeconds: 0.5,
            maxDelaySeconds: 10.0,
            jitter: false,
            retryOn: [NetworkException::class],
        );
        $this->assertSame(3, $config->maxAttempts);
        $this->assertSame(0.5, $config->baseDelaySeconds);
        $this->assertSame(10.0, $config->maxDelaySeconds);
        $this->assertFalse($config->jitter);
        $this->assertSame([NetworkException::class], $config->retryOn);
    }
}
