<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\TypeBuilder;

/**
 * Tests for TypeBuilder class.
 */
final class TypeBuilderTest extends TestCase
{
    public function testCreateBuilder(): void
    {
        $builder = TypeBuilder::type('User');

        $this->assertSame('User', $builder->getName());
    }

    public function testAddField(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('id', 'Int', nullable: false);

        $this->assertTrue($builder->hasField('id'));
        $this->assertSame(1, $builder->getFieldCount());
    }

    public function testAddMultipleFields(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('id', 'Int')
            ->field('name', 'String')
            ->field('email', 'String', nullable: true);

        $this->assertSame(3, $builder->getFieldCount());
    }

    public function testScalarField(): void
    {
        $builder = TypeBuilder::type('User')
            ->scalarField('id', 'Int');

        $field = $builder->getField('id');
        $this->assertFalse($field->nullable);
        $this->assertSame('Int', $field->type);
    }

    public function testOptionalField(): void
    {
        $builder = TypeBuilder::type('User')
            ->optionalField('email', 'String');

        $field = $builder->getField('email');
        $this->assertTrue($field->nullable);
    }

    public function testListField(): void
    {
        $builder = TypeBuilder::type('Query')
            ->listField('users', 'User');

        $field = $builder->getField('users');
        $this->assertTrue($field->isList);
        $this->assertFalse($field->nullable);
    }

    public function testOptionalListField(): void
    {
        $builder = TypeBuilder::type('Query')
            ->optionalListField('products', 'Product');

        $field = $builder->getField('products');
        $this->assertTrue($field->isList);
        $this->assertTrue($field->nullable);
    }

    public function testFieldWithDescription(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('name', 'String', description: 'User full name');

        $field = $builder->getField('name');
        $this->assertSame('User full name', $field->description);
    }

    public function testFieldWithResolver(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('fullName', 'String')
            ->withResolver('fullName', 'getFullName');

        $field = $builder->getField('fullName');
        $this->assertSame('getFullName', $field->customResolver);
    }

    public function testTypeDescription(): void
    {
        $builder = TypeBuilder::type('User')
            ->description('User in the system');

        $this->assertSame('User in the system', $builder->getDescription());
    }

    public function testFluentInterface(): void
    {
        $builder = TypeBuilder::type('User');
        $result1 = $builder->field('id', 'Int');
        $result2 = $builder->description('A user');
        $result3 = $builder->optionalField('email', 'String');

        $this->assertSame($builder, $result1);
        $this->assertSame($builder, $result2);
        $this->assertSame($builder, $result3);
    }

    public function testGetFields(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('id', 'Int')
            ->field('name', 'String');

        $fields = $builder->getFields();
        $this->assertCount(2, $fields);
        $this->assertArrayHasKey('id', $fields);
        $this->assertArrayHasKey('name', $fields);
    }

    public function testGetNonExistentField(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('id', 'Int');

        $this->assertNull($builder->getField('nonexistent'));
    }

    public function testFieldCountZero(): void
    {
        $builder = TypeBuilder::type('Empty');

        $this->assertSame(0, $builder->getFieldCount());
    }

    public function testComplexSchema(): void
    {
        $builder = TypeBuilder::type('Query')
            ->scalarField('hello', 'String', 'A simple hello')
            ->field('user', 'User')
            ->listField('users', 'User', 'All users')
            ->optionalField('totalUsers', 'Int')
            ->withResolver('totalUsers', 'getTotalUserCount');

        $this->assertSame(4, $builder->getFieldCount());

        $hello = $builder->getField('hello');
        $this->assertFalse($hello->nullable);
        $this->assertSame('A simple hello', $hello->description);

        $totalUsers = $builder->getField('totalUsers');
        $this->assertTrue($totalUsers->nullable);
        $this->assertSame('getTotalUserCount', $totalUsers->customResolver);

        $users = $builder->getField('users');
        $this->assertTrue($users->isList);
    }
}
