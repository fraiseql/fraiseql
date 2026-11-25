<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Exceptions\FraiseQLException;
use FraiseQL\FraiseQLClient;
use PHPUnit\Framework\TestCase;

/**
 * A request raises no deprecation on the PHP versions the SDK supports.
 *
 * PHP 8.5 deprecates curl_close(), which has done nothing since PHP 8.0. A request to a
 * closed port runs the whole request path, including what follows curl_exec(), and fails
 * fast without a server.
 */
final class FraiseQLClientDeprecationTest extends TestCase
{
    public function testARequestRaisesNoDeprecation(): void
    {
        $deprecations = [];
        set_error_handler(
            static function (int $errno, string $message) use (&$deprecations): bool {
                $deprecations[] = $message;
                return true;
            },
            E_DEPRECATED | E_USER_DEPRECATED,
        );
        try {
            (new FraiseQLClient('http://127.0.0.1:9/graphql', timeout: 2.0))->query('{ x }');
            $this->fail('a request to a closed port must fail');
        } catch (FraiseQLException) {
            // The connection is refused: the request path ran to the end.
        } finally {
            restore_error_handler();
        }

        $this->assertSame([], $deprecations);
    }
}
