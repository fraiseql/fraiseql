<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\StaticAPI;
use FraiseQL\SchemaRegistry;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;

/**
 * Tests for StaticAPI class.
 */
final class StaticAPITest extends TestCase
{
    protected function tearDown(): void
    {
        StaticAPI::clear();
        parent::tearDown();
    }

    public function testRegisterType(): void
    {
        StaticAPI::register(ApiUserType::class);

        $this->assertTrue(StaticAPI::hasType('ApiUser'));
    }

    public function testGetType(): void
    {
        StaticAPI::register(ApiUserType::class);

        $type = StaticAPI::getType('ApiUser');
        $this->assertNotNull($type);
    }

    public function testGetTypeFields(): void
    {
        StaticAPI::register(ApiUserType::class);

        $fields = StaticAPI::getTypeFields('ApiUser');
        $this->assertCount(2, $fields);
    }

    public function testGetField(): void
    {
        StaticAPI::register(ApiUserType::class);

        $field = StaticAPI::getField('ApiUser', 'id');
        $this->assertNotNull($field);
        $this->assertSame('id', $field->name);
    }

    public function testTypeBuilderIntegration(): void
    {
        $builder = StaticAPI::type('Query')
            ->field('hello', 'String')
            ->description('Root query type');

        StaticAPI::registerBuilder($builder);

        // Verify the type is registered and getType returns a proper GraphQLType
        $this->assertTrue(StaticAPI::hasType('Query'));

        $type = StaticAPI::getType('Query');
        $this->assertNotNull($type, 'getType() should return GraphQLType for builder-registered types');
        $this->assertInstanceOf(GraphQLType::class, $type);
        $this->assertSame('Query', $type->name);
        $this->assertSame('Root query type', $type->description);

        // Verify fields are also registered
        $fields = StaticAPI::getTypeFields('Query');
        $this->assertCount(1, $fields);
        $this->assertArrayHasKey('hello', $fields);
    }

    public function testTypeBuilderWithoutDescription(): void
    {
        $builder = StaticAPI::type('Mutation')
            ->field('createUser', 'User');

        StaticAPI::registerBuilder($builder);

        $type = StaticAPI::getType('Mutation');
        $this->assertNotNull($type);
        $this->assertSame('Mutation', $type->name);
        $this->assertNull($type->description);
    }

    public function testGetTypeNames(): void
    {
        StaticAPI::register(ApiUserType::class);
        StaticAPI::register(ApiProductType::class);

        $typeNames = StaticAPI::getTypeNames();
        $this->assertContains('ApiUser', $typeNames);
        $this->assertContains('ApiProduct', $typeNames);
    }

    public function testGetTypeNameForClass(): void
    {
        StaticAPI::register(ApiUserType::class);

        $typeName = StaticAPI::getTypeNameForClass(ApiUserType::class);
        $this->assertSame('ApiUser', $typeName);
    }

    public function testClearRegistry(): void
    {
        StaticAPI::register(ApiUserType::class);
        $this->assertTrue(StaticAPI::hasType('ApiUser'));

        StaticAPI::clear();
        $this->assertFalse(StaticAPI::hasType('ApiUser'));
    }

    public function testMultipleTypeRegistration(): void
    {
        StaticAPI::register(ApiUserType::class);
        StaticAPI::register(ApiProductType::class);

        $this->assertTrue(StaticAPI::hasType('ApiUser'));
        $this->assertTrue(StaticAPI::hasType('ApiProduct'));

        $userFields = StaticAPI::getTypeFields('ApiUser');
        $productFields = StaticAPI::getTypeFields('ApiProduct');

        $this->assertCount(2, $userFields);
        $this->assertCount(2, $productFields);
    }
}

// Test fixtures
#[GraphQLType(name: 'ApiUser')]
final class ApiUserType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;
}

#[GraphQLType(name: 'ApiProduct')]
final class ApiProductType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $title;
}
