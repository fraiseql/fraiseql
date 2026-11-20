<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use FraiseQL\TypeBuilder;
use PHPUnit\Framework\TestCase;

/**
 * `scopedLikeParent` authoring: a field that embeds a scoped type declares that the type
 * holding it is scoped the same way. These tests follow the declaration to the exported
 * schema, where only the declaring field carries the key, and refuse it on an input field
 * at export, as the Python and TypeScript SDKs do.
 */
final class ScopedLikeParentTest extends TestCase
{
    protected function setUp(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    /**
     * @param list<array<string, mixed>> $items
     * @return array<string, mixed>
     */
    private static function named(array $items, string $name): array
    {
        foreach ($items as $item) {
            if ($item['name'] === $name) {
                return $item;
            }
        }

        self::fail(sprintf('%s is absent from the exported schema', $name));
    }

    public function testOnlyTheDeclaringFieldIsExportedAsScopedLikeParent(): void
    {
        StaticAPI::register(ScopedShipment::class);
        StaticAPI::register(ScopedParcel::class);

        $types  = SchemaExporter::toArray()['types'];
        $fields = self::named($types, 'ScopedShipment')['fields'];
        self::assertSame(
            ['name' => 'order', 'type' => 'Order', 'nullable' => false, 'scoped_like_parent' => true],
            self::named($fields, 'order'),
        );
        self::assertArrayNotHasKey('scoped_like_parent', self::named($fields, 'id'));
        self::assertArrayNotHasKey('scoped_like_parent', self::named($fields, 'previousOrder'));
        $other = self::named($types, 'ScopedParcel')['fields'];
        self::assertArrayNotHasKey('scoped_like_parent', self::named($other, 'order'));
    }

    public function testTheTypeBuilderDeclaresAScopedLikeParentField(): void
    {
        $builder = TypeBuilder::type('Shipment')->sqlSource('v_shipment');
        $builder->field('id', 'ID')
            ->field('order', 'Order', scopedLikeParent: true)
            ->field('previousOrder', 'Order', nullable: true, scopedLikeParent: false);
        $builder->register();

        $fields = self::named(SchemaExporter::toArray()['types'], 'Shipment')['fields'];
        self::assertTrue(self::named($fields, 'order')['scoped_like_parent'] ?? false);
        self::assertArrayNotHasKey('scoped_like_parent', self::named($fields, 'id'));
        self::assertArrayNotHasKey('scoped_like_parent', self::named($fields, 'previousOrder'));
    }

    public function testScopedLikeParentIsRefusedOnAnInputTypeField(): void
    {
        StaticAPI::register(ScopedShipmentInput::class);

        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/ScopedShipmentInput\.order.*scoped_like_parent/');
        SchemaExporter::toArray();
    }

    public function testScopedLikeParentIsRefusedOnARegisteredInputField(): void
    {
        // The key is outside the documented field shape, which is the point: it is refused,
        // not passed through to the compiler.
        $fields = [['name' => 'order', 'type' => 'ID', 'nullable' => false, 'scoped_like_parent' => true]];
        SchemaRegistry::getInstance()->registerInputType('ShipOrderInput', $fields);

        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/ShipOrderInput\.order.*scoped_like_parent/');
        SchemaExporter::toArray();
    }
}

#[GraphQLType(sqlSource: 'v_scoped_shipment')]
final class ScopedShipment
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'Order', nullable: false, scopedLikeParent: true)]
    public string $order;

    #[GraphQLField(type: 'Order', nullable: true)]
    public ?string $previousOrder;
}

#[GraphQLType(sqlSource: 'v_scoped_parcel')]
final class ScopedParcel
{
    #[GraphQLField(type: 'Order', nullable: false)]
    public string $order;
}

#[GraphQLType(isInput: true)]
final class ScopedShipmentInput
{
    #[GraphQLField(type: 'ID', nullable: false, scopedLikeParent: true)]
    public string $order;
}
