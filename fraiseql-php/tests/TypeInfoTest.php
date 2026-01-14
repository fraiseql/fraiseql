<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\TypeInfo;

/**
 * Tests for TypeInfo class.
 */
final class TypeInfoTest extends TestCase
{
    public function testCreateFromBuiltinType(): void
    {
        $typeInfo = TypeInfo::fromString('int');

        $this->assertSame('int', $typeInfo->phpType);
        $this->assertSame('Int', $typeInfo->graphQLType);
        $this->assertFalse($typeInfo->isNullable);
        $this->assertFalse($typeInfo->isList);
    }

    public function testCreateFromNullableType(): void
    {
        $typeInfo = TypeInfo::fromString('?string');

        $this->assertSame('string', $typeInfo->phpType);
        $this->assertSame('String', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isNullable);
        $this->assertFalse($typeInfo->isList);
    }

    public function testCreateFromUnionWithNull(): void
    {
        $typeInfo = TypeInfo::fromString('bool|null');

        $this->assertSame('bool', $typeInfo->phpType);
        $this->assertSame('Boolean', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isNullable);
    }

    public function testCreateFromArrayType(): void
    {
        $typeInfo = TypeInfo::fromString('int[]');

        $this->assertSame('int', $typeInfo->phpType);
        $this->assertSame('Int', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isList);
        $this->assertFalse($typeInfo->isNullable);
    }

    public function testCreateFromGenericArrayType(): void
    {
        $typeInfo = TypeInfo::fromString('array<string>');

        $this->assertSame('string', $typeInfo->phpType);
        $this->assertSame('String', $typeInfo->graphQLType);
        $this->assertTrue($typeInfo->isList);
    }

    public function testCreateFromCustomClassName(): void
    {
        $typeInfo = TypeInfo::fromString('User');

        $this->assertSame('User', $typeInfo->phpType);
        $this->assertSame('User', $typeInfo->graphQLType);
        $this->assertFalse($typeInfo->isNullable);
        $this->assertFalse($typeInfo->isList);
    }

    public function testPhpTypeToGraphQLConversion(): void
    {
        $this->assertSame('Int', TypeInfo::fromString('int')->graphQLType);
        $this->assertSame('String', TypeInfo::fromString('string')->graphQLType);
        $this->assertSame('Boolean', TypeInfo::fromString('bool')->graphQLType);
        $this->assertSame('Float', TypeInfo::fromString('float')->graphQLType);
    }

    public function testToGraphQLTypeString(): void
    {
        // Non-nullable scalar
        $this->assertSame('Int!', TypeInfo::fromString('int')->toGraphQLTypeString());

        // Nullable scalar
        $this->assertSame('String', TypeInfo::fromString('?string')->toGraphQLTypeString());

        // Non-nullable list
        $this->assertSame('[Int!]', TypeInfo::fromString('int[]')->toGraphQLTypeString());

        // Nullable list
        $this->assertSame('[String]', TypeInfo::fromString('?string[]')->toGraphQLTypeString());
    }

    public function testToGraphQLTypeStringWithNonNullList(): void
    {
        $this->assertSame('[Int!]!', TypeInfo::fromString('int[]')->toGraphQLTypeString(true));
        $this->assertSame('[String]!', TypeInfo::fromString('?string[]')->toGraphQLTypeString(true));
    }

    public function testIsCustomType(): void
    {
        $this->assertFalse(TypeInfo::fromString('int')->isCustomType());
        $this->assertFalse(TypeInfo::fromString('string')->isCustomType());
        $this->assertTrue(TypeInfo::fromString('User')->isCustomType());
        $this->assertTrue(TypeInfo::fromString('Product')->isCustomType());
    }

    public function testConstructorWithDescription(): void
    {
        $typeInfo = new TypeInfo(
            phpType: 'string',
            graphQLType: 'String',
            description: 'User full name',
        );

        $this->assertSame('User full name', $typeInfo->description);
    }

    public function testConstructorWithCustomResolver(): void
    {
        $typeInfo = new TypeInfo(
            phpType: 'int',
            graphQLType: 'Int',
            customResolver: 'getComputedAge',
        );

        $this->assertSame('getComputedAge', $typeInfo->customResolver);
    }
}
