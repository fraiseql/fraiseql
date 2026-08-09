<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;
use FraiseQL\TypeBuilder;
use PHPUnit\Framework\TestCase;

/**
 * A type authored with the fluent TypeBuilder must export the same facts as one
 * authored with #[GraphQLType] attributes (issue #952).
 *
 * `TypeBuilder::register()` stored `sqlSource` and `isError` in a side table
 * (`SchemaRegistry::setTypeMeta`) while constructing the `GraphQLType` attribute with
 * only `name` and `description`. `SchemaExporter` — the exporter `vendor/bin/fraiseql
 * export` runs — reads the attribute, so every builder-authored type reached the
 * compiler with **no `sql_source` and no `is_error`**: a query would be compiled
 * against nothing, and an error type would be indistinguishable from a data type.
 *
 * Three gates missed it. The conformance suite authors through attributes, so it never
 * walked this path; the parity generator uses the builder but exported through a second
 * serializer (`StaticAPI::exportSchema`, since removed) that read the side table; and
 * the parity comparison crashed on that serializer's field shape before comparing
 * anything at all.
 */
final class TypeBuilderExportTest extends TestCase
{
    protected function setUp(): void
    {
        parent::setUp();
        StaticAPI::clear();
    }

    protected function tearDown(): void
    {
        StaticAPI::clear();
        parent::tearDown();
    }

    /** @return array<string, mixed> */
    private function exportedType(string $name): array
    {
        foreach (SchemaExporter::toArray()['types'] as $type) {
            if ($type['name'] === $name) {
                return $type;
            }
        }
        $this->fail("type '$name' is absent from the exported schema");
    }

    public function testBuilderSqlSourceReachesTheExportedSchema(): void
    {
        TypeBuilder::type('User')
            ->sqlSource('v_user')
            ->field('id', 'ID', nullable: false)
            ->register();

        $this->assertSame('v_user', $this->exportedType('User')['sql_source'] ?? null);
    }

    public function testBuilderIsErrorReachesTheExportedSchema(): void
    {
        TypeBuilder::type('UserNotFound')
            ->sqlSource('v_user_not_found')
            ->isError(true)
            ->field('message', 'String', nullable: false)
            ->register();

        $this->assertTrue($this->exportedType('UserNotFound')['is_error'] ?? false);
    }

    public function testBuilderDescriptionReachesTheExportedSchema(): void
    {
        TypeBuilder::type('Order')
            ->description('A customer order')
            ->sqlSource('v_order')
            ->field('id', 'ID', nullable: false)
            ->register();

        $this->assertSame('A customer order', $this->exportedType('Order')['description'] ?? null);
    }

    public function testNonErrorBuilderTypeCarriesNoIsErrorKey(): void
    {
        TypeBuilder::type('Order')
            ->sqlSource('v_order')
            ->field('id', 'ID', nullable: false)
            ->register();

        $this->assertArrayNotHasKey('is_error', $this->exportedType('Order'));
    }

    public function testBuilderFieldsAreExportedAsAList(): void
    {
        TypeBuilder::type('User')
            ->sqlSource('v_user')
            ->field('id', 'ID', nullable: false)
            ->field('email', 'String', nullable: false)
            ->register();

        $fields = $this->exportedType('User')['fields'];

        $this->assertSame([0, 1], array_keys($fields), 'fields must be a list, not name-keyed');
        $this->assertSame('id', $fields[0]['name']);
        $this->assertSame('email', $fields[1]['name']);
    }
}
