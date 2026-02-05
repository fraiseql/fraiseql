<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use ReflectionClass;

/**
 * Tests for GraphQL PHP 8 Attributes.
 */
final class AttributesTest extends TestCase
{
    public function testGraphQLTypeAttributeCreation(): void
    {
        $attribute = new GraphQLType(name: 'User');

        $this->assertSame('User', $attribute->name);
        $this->assertNull($attribute->description);
        $this->assertFalse($attribute->isInput);
    }

    public function testGraphQLTypeAttributeWithDescription(): void
    {
        $attribute = new GraphQLType(
            name: 'Product',
            description: 'Product information',
        );

        $this->assertSame('Product', $attribute->name);
        $this->assertSame('Product information', $attribute->description);
    }

    public function testGraphQLTypeAttributeAsInputType(): void
    {
        $attribute = new GraphQLType(
            name: 'UserInput',
            isInput: true,
        );

        $this->assertTrue($attribute->isInput);
    }

    public function testGraphQLFieldAttributeCreation(): void
    {
        $attribute = new GraphQLField(type: 'String');

        $this->assertSame('String', $attribute->type);
        $this->assertNull($attribute->description);
        $this->assertFalse($attribute->nullable);
        $this->assertNull($attribute->resolver);
    }

    public function testGraphQLFieldAttributeWithAllProperties(): void
    {
        $attribute = new GraphQLField(
            type: 'Int',
            description: 'User age',
            nullable: true,
            resolver: 'calculateAge',
        );

        $this->assertSame('Int', $attribute->type);
        $this->assertSame('User age', $attribute->description);
        $this->assertTrue($attribute->nullable);
        $this->assertSame('calculateAge', $attribute->resolver);
    }

    public function testAttributeOnReflectedClass(): void
    {
        $class = new ReflectionClass(ExampleGraphQLType::class);
        $attributes = $class->getAttributes(GraphQLType::class);

        $this->assertCount(1, $attributes);

        /** @var GraphQLType $attribute */
        $attribute = $attributes[0]->newInstance();
        $this->assertSame('Example', $attribute->name);
        $this->assertSame('Example type', $attribute->description);
    }

    public function testAttributeOnReflectedProperty(): void
    {
        $class = new ReflectionClass(ExampleGraphQLType::class);
        $property = $class->getProperty('id');

        $attributes = $property->getAttributes(GraphQLField::class);

        $this->assertCount(1, $attributes);

        /** @var GraphQLField $attribute */
        $attribute = $attributes[0]->newInstance();
        $this->assertSame('Int', $attribute->type);
        $this->assertFalse($attribute->nullable);
    }

    public function testMultipleFieldAttributes(): void
    {
        $class = new ReflectionClass(ExampleGraphQLType::class);

        $idProperty = $class->getProperty('id');
        $idAttributes = $idProperty->getAttributes(GraphQLField::class);
        $this->assertCount(1, $idAttributes);

        $nameProperty = $class->getProperty('name');
        $nameAttributes = $nameProperty->getAttributes(GraphQLField::class);
        $this->assertCount(1, $nameAttributes);

        /** @var GraphQLField $nameAttribute */
        $nameAttribute = $nameAttributes[0]->newInstance();
        $this->assertSame('User name', $nameAttribute->description);
    }
}

// Test fixtures
#[GraphQLType(name: 'Example', description: 'Example type')]
final class ExampleGraphQLType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(description: 'User name')]
    public string $name;
}
