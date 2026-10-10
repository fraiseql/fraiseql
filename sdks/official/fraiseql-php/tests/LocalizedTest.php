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
 * `localized` authoring (#1527): a String field, input field or mutation argument stored
 * as a locale map. These tests follow the declaration to the exported schema, and refuse
 * it on anything but a String at export, as the Python and TypeScript SDKs refuse
 * `Localized[int]`.
 */
final class LocalizedTest extends TestCase
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

    public function testALocalizedFieldIsExportedAsLocalized(): void
    {
        StaticAPI::register(LocalizedProduct::class);
        SchemaRegistry::getInstance()->registerInputType('CreateProductInput', [
            ['name' => 'name', 'type' => 'String', 'nullable' => false, 'localized' => true],
        ]);

        $schema = SchemaExporter::toArray();
        $fields = self::named($schema['types'], 'LocalizedProduct')['fields'];
        self::assertTrue(self::named($fields, 'name')['localized'] ?? false);
        self::assertArrayNotHasKey('localized', self::named($fields, 'sku'));
        $inputs = self::named($schema['input_types'], 'CreateProductInput')['fields'];
        self::assertTrue(self::named($inputs, 'name')['localized'] ?? false);
    }

    public function testTheTypeBuilderDeclaresALocalizedField(): void
    {
        $builder = TypeBuilder::type('Article')->sqlSource('v_article');
        $builder->field('id', 'ID')->field('title', 'String', localized: true)->field('slug', 'String');
        $builder->register();

        $fields = self::named(SchemaExporter::toArray()['types'], 'Article')['fields'];
        self::assertTrue(self::named($fields, 'title')['localized'] ?? false);
        self::assertArrayNotHasKey('localized', self::named($fields, 'slug'));
    }

    public function testALocalizedMutationArgumentIsExportedAsLocalized(): void
    {
        StaticAPI::mutation('createProduct')
            ->returnType('Product')
            ->sqlSource('fn_create_product')
            ->operation('insert')
            ->argument('sku', 'String', nullable: false)
            ->argument('displayName', 'String', nullable: true, localized: true)
            ->register();

        $mutation = self::named(SchemaExporter::toArray()['mutations'], 'createProduct');
        self::assertSame(
            ['name' => 'displayName', 'type' => 'String', 'nullable' => true, 'localized' => true],
            self::named($mutation['arguments'], 'displayName'),
        );
        self::assertArrayNotHasKey('localized', self::named($mutation['arguments'], 'sku'));
    }

    public function testACrudInputFieldIsLocalizedAsItsFieldIs(): void
    {
        StaticAPI::register(LocalizedArticle::class);

        $inputs = SchemaExporter::toArray()['input_types'];
        foreach (['CreateLocalizedArticleInput', 'UpdateLocalizedArticleInput'] as $input) {
            self::assertTrue(self::named(self::named($inputs, $input)['fields'], 'title')['localized'] ?? false, $input);
        }
    }

    public function testLocalizedIsRefusedOnAFieldThatIsNotAString(): void
    {
        StaticAPI::register(LocalizedPriced::class);

        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/price.*only a String can be localized/');
        SchemaExporter::toArray();
    }

    public function testLocalizedIsRefusedOnANonStringInputField(): void
    {
        SchemaRegistry::getInstance()->registerInputType('PriceInput', [
            ['name' => 'amount', 'type' => 'Float', 'nullable' => false, 'localized' => true],
        ]);

        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/amount.*only a String can be localized/');
        SchemaExporter::toArray();
    }

    public function testLocalizedIsRefusedOnANonStringArgument(): void
    {
        StaticAPI::mutation('setPrice')
            ->returnType('Product')
            ->sqlSource('fn_set_price')
            ->operation('update')
            ->argument('amount', 'Float', nullable: false, localized: true)
            ->register();

        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/amount.*only a String can be localized/');
        SchemaExporter::toArray();
    }
}

#[GraphQLType(sqlSource: 'v_product')]
final class LocalizedProduct
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'String', nullable: true, localized: true)]
    public ?string $name;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $sku;
}

#[GraphQLType(sqlSource: 'v_article', crud: true)]
final class LocalizedArticle
{
    #[GraphQLField(type: 'Int', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false, localized: true)]
    public string $title;
}

#[GraphQLType(sqlSource: 'v_priced')]
final class LocalizedPriced
{
    #[GraphQLField(type: 'Float', nullable: false, localized: true)]
    public float $price;
}
