<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\FraiseQLException;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

enum EnumValuesOrderStatus: string
{
    case PENDING = 'pending';
    case IN_TRANSIT = 'transit';
    case SHIPPED = 'SHIPPED';
}

enum EnumValuesPriority: int
{
    case LOW = 1;
    case HIGH = 3;
}

enum EnumValuesColor
{
    case RED;
    case GREEN;
}

/**
 * #1611: a member is stored as the value PHP gives it, a backed enum's string or integer, and
 * as its name when the enum is not backed or the value is the name.
 */
final class EnumValuesTest extends TestCase
{
    protected function tearDown(): void
    {
        StaticAPI::clear();
        parent::tearDown();
    }

    /** @return list<array<string, mixed>> */
    private static function exported(string $name): array
    {
        return SchemaRegistry::getInstance()->getAllEnums()[$name]['values'];
    }

    public function testAStringBackedEnumExportsItsValues(): void
    {
        StaticAPI::enum(EnumValuesOrderStatus::class, 'The status of an order');

        $this->assertSame(
            [
                ['name' => 'PENDING', 'value' => 'pending'],
                ['name' => 'IN_TRANSIT', 'value' => 'transit'],
                ['name' => 'SHIPPED'],
            ],
            self::exported('EnumValuesOrderStatus'),
        );
        $this->assertSame(
            'The status of an order',
            SchemaRegistry::getInstance()->getAllEnums()['EnumValuesOrderStatus']['description'],
        );
    }

    public function testAnIntBackedEnumExportsItsIntegers(): void
    {
        StaticAPI::enum(EnumValuesPriority::class);

        $this->assertSame(
            [['name' => 'LOW', 'value' => 1], ['name' => 'HIGH', 'value' => 3]],
            self::exported('EnumValuesPriority'),
        );
    }

    public function testAPureEnumIsStoredByItsNames(): void
    {
        StaticAPI::enum(EnumValuesColor::class);

        $this->assertSame([['name' => 'RED'], ['name' => 'GREEN']], self::exported('EnumValuesColor'));
    }

    public function testAClassThatIsNotAnEnumIsRefused(): void
    {
        $this->expectException(FraiseQLException::class);
        $this->expectExceptionMessage('stdClass');

        StaticAPI::enum(\stdClass::class);
    }
}
