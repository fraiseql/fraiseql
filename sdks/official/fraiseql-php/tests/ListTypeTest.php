<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Schema;
use FraiseQL\TypeBuilder;
use PHPUnit\Framework\TestCase;

// A list position is non-null unless PHP marks it nullable (#1586, #1608): `?T` / `T|null`, in
// the property type for the list itself and in the docblock generic (`list<?float>`) for its
// element, at every level. PHP's type system has no element types; the docblock is where
// PHPStan, Psalm and IDEs read them.
#[GraphQLType(name: 'Series', sqlSource: 'v_series')]
final class SeriesFixture
{
    /** @var list<string> */
    #[GraphQLField]
    public array $labels;

    /** @var list<?float>|null */
    #[GraphQLField]
    public ?array $readings;

    /** @var list<list<float|null>> */
    #[GraphQLField]
    public array $grid;

    /** @var array<int, ?list<float>> */
    #[GraphQLField]
    public array $rows;

    /** @var int[] */
    #[GraphQLField]
    public array $counts;

    /** @var (string|null)[] */
    #[GraphQLField]
    public array $notes;

    // A type the attribute names is published as written: a list type, or the scalar an
    // array holds (a `Vector`).
    #[GraphQLField(type: '[Int]')]
    public array $loose;

    #[GraphQLField(type: 'Vector')]
    public array $embedding;
}

#[GraphQLType(name: 'Untyped', sqlSource: 'v_untyped')]
final class UntypedListFixture
{
    #[GraphQLField]
    public array $things;
}

final class ListTypeTest extends TestCase
{
    protected function setUp(): void
    {
        Schema::reset();
    }

    /** @return array<string, array{type: string, nullable: bool}> */
    private static function exported(string $className, string $typeName): array
    {
        Schema::registerType($className);
        $parsed = json_decode(Schema::exportTypes(true), true);
        $fields = [];
        foreach ($parsed['types'] as $type) {
            if ($type['name'] === $typeName) {
                foreach ($type['fields'] as $field) {
                    $fields[$field['name']] = ['type' => $field['type'], 'nullable' => $field['nullable']];
                }
            }
        }
        return $fields;
    }

    public function testEachListLevelIsNonNullUnlessMarkedNullable(): void
    {
        $this->assertSame([
            'labels' => ['type' => '[String!]', 'nullable' => false],
            'readings' => ['type' => '[Float]', 'nullable' => true],
            'grid' => ['type' => '[[Float]!]', 'nullable' => false],
            'rows' => ['type' => '[[Float!]]', 'nullable' => false],
            'counts' => ['type' => '[Int!]', 'nullable' => false],
            'notes' => ['type' => '[String]', 'nullable' => false],
            'loose' => ['type' => '[Int]', 'nullable' => false],
            'embedding' => ['type' => 'Vector', 'nullable' => false],
        ], self::exported(SeriesFixture::class, 'Series'));
    }

    public function testAnArrayWithNoElementTypeIsRefused(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/Untyped(Fixture|ListFixture)?::\$things.*element type/');
        self::exported(UntypedListFixture::class, 'Untyped');
    }

    public function testABuilderListFieldHasNonNullItems(): void
    {
        $builder = TypeBuilder::type('Tagged')
            ->listField('tags', 'String')
            ->optionalListField('aliases', 'String');

        $fields = $builder->getFields();

        $this->assertSame('[String!]', $fields['tags']->type);
        $this->assertSame('[String!]!', $fields['tags']->getGraphQLTypeString());
        $this->assertSame('[String!]', $fields['aliases']->type);
        $this->assertTrue($fields['aliases']->nullable);
        $this->assertSame('[String!]', $fields['aliases']->getGraphQLTypeString());
    }
}
