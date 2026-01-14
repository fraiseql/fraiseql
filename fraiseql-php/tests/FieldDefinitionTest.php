<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\FieldDefinition;

/**
 * Tests for FieldDefinition class.
 */
final class FieldDefinitionTest extends TestCase
{
    public function testCreateFieldDefinition(): void
    {
        $field = new FieldDefinition(
            name: 'id',
            type: 'Int',
            nullable: false,
            parentType: 'User',
        );

        $this->assertSame('id', $field->name);
        $this->assertSame('Int', $field->type);
        $this->assertFalse($field->nullable);
    }

    public function testFieldWithDescription(): void
    {
        $field = new FieldDefinition(
            name: 'email',
            type: 'String',
            description: 'User email address',
            parentType: 'User',
        );

        $this->assertSame('User email address', $field->description);
    }

    public function testNullableField(): void
    {
        $field = new FieldDefinition(
            name: 'email',
            type: 'String',
            nullable: true,
            parentType: 'User',
        );

        $this->assertTrue($field->nullable);
    }

    public function testListField(): void
    {
        $field = new FieldDefinition(
            name: 'tags',
            type: 'String',
            isList: true,
            parentType: 'Post',
        );

        $this->assertTrue($field->isList);
    }

    public function testGetGraphQLTypeString(): void
    {
        $field = new FieldDefinition(
            name: 'id',
            type: 'Int',
            nullable: false,
            parentType: 'User',
        );

        $this->assertSame('Int!', $field->getGraphQLTypeString());
    }

    public function testGetGraphQLTypeStringNullable(): void
    {
        $field = new FieldDefinition(
            name: 'email',
            type: 'String',
            nullable: true,
            parentType: 'User',
        );

        $this->assertSame('String', $field->getGraphQLTypeString());
    }

    public function testGetGraphQLTypeStringList(): void
    {
        $field = new FieldDefinition(
            name: 'tags',
            type: 'String',
            nullable: false,
            isList: true,
            parentType: 'Post',
        );

        $this->assertSame('[String!]', $field->getGraphQLTypeString());
    }

    public function testGetGraphQLTypeStringNullableList(): void
    {
        $field = new FieldDefinition(
            name: 'tags',
            type: 'String',
            nullable: true,
            isList: true,
            parentType: 'Post',
        );

        $this->assertSame('[String]', $field->getGraphQLTypeString());
    }

    public function testGetGraphQLTypeStringWithNonNullList(): void
    {
        $field = new FieldDefinition(
            name: 'tags',
            type: 'String',
            nullable: true,
            isList: true,
            parentType: 'Post',
        );

        $this->assertSame('[String]!', $field->getGraphQLTypeString(true));
    }

    public function testIsScalar(): void
    {
        $intField = new FieldDefinition(
            name: 'id',
            type: 'Int',
            parentType: 'User',
        );

        $this->assertTrue($intField->isScalar());

        $userField = new FieldDefinition(
            name: 'author',
            type: 'User',
            parentType: 'Post',
        );

        $this->assertFalse($userField->isScalar());
    }

    public function testIsScalarForAllTypes(): void
    {
        $this->assertTrue((new FieldDefinition('id', 'Int', parentType: 'User'))->isScalar());
        $this->assertTrue((new FieldDefinition('name', 'String', parentType: 'User'))->isScalar());
        $this->assertTrue((new FieldDefinition('active', 'Boolean', parentType: 'User'))->isScalar());
        $this->assertTrue((new FieldDefinition('price', 'Float', parentType: 'Product'))->isScalar());
        $this->assertFalse((new FieldDefinition('user', 'User', parentType: 'Post'))->isScalar());
    }

    public function testCustomResolver(): void
    {
        $field = new FieldDefinition(
            name: 'fullName',
            type: 'String',
            customResolver: 'getFullName',
            parentType: 'User',
        );

        $this->assertTrue($field->hasCustomResolver());
        $this->assertSame('getFullName', $field->customResolver);
    }

    public function testNoCustomResolver(): void
    {
        $field = new FieldDefinition(
            name: 'name',
            type: 'String',
            parentType: 'User',
        );

        $this->assertFalse($field->hasCustomResolver());
        $this->assertNull($field->customResolver);
    }

    public function testToString(): void
    {
        $field = new FieldDefinition(
            name: 'id',
            type: 'Int',
            nullable: false,
            parentType: 'User',
        );

        $this->assertSame('User.id: Int!', (string)$field);
    }

    public function testReadonly(): void
    {
        $field = new FieldDefinition(
            name: 'id',
            type: 'Int',
            parentType: 'User',
        );

        $this->assertSame('id', $field->name);

        // Verify readonly by attempting to set property
        // This would throw an error in PHP 8.2+
        try {
            $field->name = 'newId';
            $this->fail('Should not be able to modify readonly property');
        } catch (\Error $e) {
            $this->assertStringContainsString('readonly', $e->getMessage());
        }
    }
}
