<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Scalars;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

/**
 * `Scalars::BIG_INT` declares a signed 64-bit integer, served as the string of its digits.
 * PHP's `int` is the type for small values, so it stays `Int`, which is 32 bits and refuses
 * a value past that at read time.
 */
final class BigIntTest extends TestCase
{
    protected function setUp(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    public function testTheMarkerIsTheBuiltInScalar(): void
    {
        self::assertSame('BigInt', Scalars::BIG_INT);
        self::assertTrue(Scalars::isScalarType('BigInt'));
        self::assertContains('BigInt', Scalars::getScalarNames());
    }

    public function testAMarkedFieldAndArgumentExportAsBigIntAndIntStaysInt(): void
    {
        StaticAPI::register(BigIntLedger::class);
        StaticAPI::query('ledgers')
            ->returnType('BigIntLedger')
            ->returnsList(true)
            ->sqlSource('v_ledger')
            ->argument('accountNumber', Scalars::BIG_INT, nullable: false)
            ->register();

        $schema = SchemaExporter::toArray();
        $fields = array_column($schema['types'][0]['fields'], 'type', 'name');
        self::assertSame(['amount' => 'BigInt', 'balance' => 'BigInt', 'count' => 'Int'], $fields);
        self::assertSame('BigInt', $schema['queries'][0]['arguments'][0]['type']);
    }
}

#[GraphQLType(name: 'BigIntLedger', sqlSource: 'v_ledger')]
final class BigIntLedger
{
    #[GraphQLField(type: Scalars::BIG_INT, nullable: false)]
    public int $amount;

    #[GraphQLField(type: Scalars::BIG_INT, nullable: true)]
    public ?int $balance;

    #[GraphQLField]
    public int $count;
}
