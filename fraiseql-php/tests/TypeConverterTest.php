<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\TypeConverter;
use FraiseQL\TypeInfo;
use ReflectionClass;

/**
 * Tests for TypeConverter class.
 */
final class TypeConverterTest extends TestCase
{
    public function testConvertFromTypeString(): void
    {
        $typeInfo = TypeConverter::fromTypeString('int');

        $this->assertSame('int', $typeInfo->phpType);
        $this->assertSame('Int', $typeInfo->graphQLType);
        $this->assertFalse($typeInfo->isNullable);
    }

    public function testConvertNullableTypeString(): void
    {
        $typeInfo = TypeConverter::fromTypeString('?bool');

        $this->assertSame('bool', $typeInfo->phpType);
        $this->assertSame('Boolean', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isNullable);
    }

    public function testConvertArrayTypeString(): void
    {
        $typeInfo = TypeConverter::fromTypeString('string[]');

        $this->assertSame('string', $typeInfo->phpType);
        $this->assertSame('String', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isList);
    }

    public function testIsScalarType(): void
    {
        $this->assertTrue(TypeConverter::isScalarType('int'));
        $this->assertTrue(TypeConverter::isScalarType('string'));
        $this->assertTrue(TypeConverter::isScalarType('bool'));
        $this->assertTrue(TypeConverter::isScalarType('float'));
        $this->assertTrue(TypeConverter::isScalarType('double'));
        $this->assertTrue(TypeConverter::isScalarType('mixed'));

        $this->assertFalse(TypeConverter::isScalarType('User'));
        $this->assertFalse(TypeConverter::isScalarType('Product'));
        $this->assertFalse(TypeConverter::isScalarType('array'));
    }

    public function testConvertFromReflectionProperty(): void
    {
        $class = new ReflectionClass(TestUser::class);
        $property = $class->getProperty('id');

        $typeInfo = TypeConverter::fromReflectionProperty($property);

        $this->assertSame('int', $typeInfo->phpType);
        $this->assertSame('Int', $typeInfo->graphQLType);
        $this->assertFalse($typeInfo->isNullable);
    }

    public function testConvertFromReflectionPropertyWithNullableType(): void
    {
        $class = new ReflectionClass(TestUser::class);
        $property = $class->getProperty('email');

        $typeInfo = TypeConverter::fromReflectionProperty($property);

        $this->assertSame('string', $typeInfo->phpType);
        $this->assertSame('String', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isNullable);
    }

    public function testConvertFromReflectionPropertyWithDescription(): void
    {
        $class = new ReflectionClass(TestUserWithAttributes::class);
        $property = $class->getProperty('name');

        $typeInfo = TypeConverter::fromReflectionProperty($property);

        $this->assertSame('User full name', $typeInfo->description);
    }

    public function testMultipleTypeConversions(): void
    {
        $conversions = [
            'int' => ['Int', false, false],
            'string' => ['String', false, false],
            'bool' => ['Boolean', false, false],
            'float' => ['Float', false, false],
            '?int' => ['Int', true, false],
            'int[]' => ['Int', false, true],
            '?string[]' => ['String', true, true],
        ];

        foreach ($conversions as $phpType => [$expectedGraphQL, $expectedNullable, $expectedList]) {
            $typeInfo = TypeConverter::fromTypeString($phpType);
            $this->assertSame($expectedGraphQL, $typeInfo->graphQLType, "Failed for $phpType");
            $this->assertSame($expectedNullable, $typeInfo->isNullable, "Nullable check failed for $phpType");
            $this->assertSame($expectedList, $typeInfo->isList, "List check failed for $phpType");
        }
    }
}

// Test fixtures
class TestUser
{
    public int $id;
    public string $name;
    public ?string $email;
}

class TestUserWithAttributes
{
    #[\FraiseQL\Attributes\GraphQLField(description: 'User full name')]
    public string $name;
}
