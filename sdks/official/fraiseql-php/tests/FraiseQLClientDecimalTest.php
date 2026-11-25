<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\FraiseQLClient;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * A Decimal travels as the string of its exact digits.
 *
 * The server takes a Decimal argument as a string such as "1.50", or an exact integer,
 * and refuses a number with a fraction. PHP has no decimal type: a string keeps every
 * digit both ways, and a float is sent as a JSON number, which the server refuses for a
 * Decimal. Runs the real client (curl) against PHP's built-in server, which echoes the
 * variables back and records the raw body.
 */
final class FraiseQLClientDecimalTest extends TestCase
{
    /** @var resource|null */
    private static $server = null;
    private static string $url = '';
    private static string $dir = '';

    public static function setUpBeforeClass(): void
    {
        self::$dir = sys_get_temp_dir() . '/fraiseql-decimal-' . bin2hex(random_bytes(4));
        mkdir(self::$dir);
        file_put_contents(self::$dir . '/router.php', <<<'PHP'
            <?php
            $body = file_get_contents('php://input');
            file_put_contents(__DIR__ . '/sent.json', $body);
            $variables = json_decode($body, true)['variables'] ?? [];
            header('Content-Type: application/json');
            echo '{"data":{"echo":' . json_encode((object) $variables) . '}}';
            PHP);

        $socket = stream_socket_server('tcp://127.0.0.1:0');
        $port = (int) substr(strrchr(stream_socket_get_name($socket, false), ':'), 1);
        fclose($socket);
        self::$url = "http://127.0.0.1:{$port}/graphql";
        self::$server = proc_open(
            [PHP_BINARY, '-S', "127.0.0.1:{$port}", self::$dir . '/router.php'],
            [1 => ['file', '/dev/null', 'w'], 2 => ['file', '/dev/null', 'w']],
            $pipes,
        );
        for ($i = 0; $i < 100; $i++) {
            $probe = @fsockopen('127.0.0.1', $port);
            if ($probe !== false) {
                fclose($probe);
                return;
            }
            usleep(50_000);
        }
        self::fail('the built-in server did not start');
    }

    public static function tearDownAfterClass(): void
    {
        if (self::$server !== null) {
            proc_terminate(self::$server);
            proc_close(self::$server);
        }
        array_map('unlink', glob(self::$dir . '/*') ?: []);
        @rmdir(self::$dir);
    }

    /** @return array{0: string, 1: array<string, mixed>} the raw body sent, and the data read back */
    private function roundTrip(array $variables): array
    {
        $client = new FraiseQLClient(self::$url, timeout: 5.0);
        $data = $client->query('query($a: Decimal) { echo(a: $a) }', $variables);
        return [(string) file_get_contents(self::$dir . '/sent.json'), $data];
    }

    /** @return iterable<string, array{string}> */
    public static function digits(): iterable
    {
        foreach (['1.50', '100.00', '12345678901234567890.12', '123456789012345678901234567890', '-0.00'] as $d) {
            yield $d => [$d];
        }
    }

    #[DataProvider('digits')]
    public function testADecimalStringIsSentAndReadAsItsExactDigits(string $digits): void
    {
        [$sent, $data] = $this->roundTrip(['a' => $digits]);
        $this->assertStringContainsString('"a":"' . $digits . '"', $sent);
        $this->assertSame($digits, $data['echo']['a']);
    }

    public function testAFloatIsSentAsANumberWhichADecimalArgumentRefuses(): void
    {
        [$sent] = $this->roundTrip(['a' => 1.50]);
        $this->assertStringContainsString('"a":1.5', $sent);
    }
}
