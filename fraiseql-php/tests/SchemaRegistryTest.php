<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaRegistry;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\FraiseQLException;

/**
 * Tests for SchemaRegistry class.
 */
final class SchemaRegistryTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testSingletonInstance(): void
    {
        $instance1 = SchemaRegistry::getInstance();
        $instance2 = SchemaRegistry::getInstance();

        $this->assertSame($instance1, $instance2);
    }

    public function testRegisterType(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);

        $this->assertTrue(SchemaRegistry::getInstance()->hasType('SimpleUser'));
    }

    public function testGetRegisteredType(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);

        $type = SchemaRegistry::getInstance()->getType('SimpleUser');
        $this->assertNotNull($type);
        $this->assertSame('SimpleUser', $type->name);
    }

    public function testGetTypeFields(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);

        $fields = SchemaRegistry::getInstance()->getTypeFields('SimpleUser');
        $this->assertCount(3, $fields);
        $this->assertArrayHasKey('id', $fields);
        $this->assertArrayHasKey('name', $fields);
        $this->assertArrayHasKey('email', $fields);
    }

    public function testGetSpecificField(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);

        $field = SchemaRegistry::getInstance()->getField('SimpleUser', 'id');
        $this->assertNotNull($field);
        $this->assertSame('id', $field->name);
        $this->assertSame('Int', $field->type);
        $this->assertFalse($field->nullable);
    }

    public function testGetNullableField(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);

        $field = SchemaRegistry::getInstance()->getField('SimpleUser', 'email');
        $this->assertNotNull($field);
        $this->assertTrue($field->nullable);
    }

    public function testGetAllTypeNames(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);
        SchemaRegistry::getInstance()->register(SimpleProductType::class);

        $typeNames = SchemaRegistry::getInstance()->getTypeNames();
        $this->assertContains('SimpleUser', $typeNames);
        $this->assertContains('SimpleProduct', $typeNames);
    }

    public function testGetTypeNameForClass(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);

        $typeName = SchemaRegistry::getInstance()->getTypeNameForClass(SimpleUserType::class);
        $this->assertSame('SimpleUser', $typeName);
    }

    public function testRegisterWithoutAttributeThrows(): void
    {
        $this->expectException(FraiseQLException::class);
        SchemaRegistry::getInstance()->register(UnattributedClass::class);
    }

    public function testClearRegistry(): void
    {
        SchemaRegistry::getInstance()->register(SimpleUserType::class);
        $this->assertTrue(SchemaRegistry::getInstance()->hasType('SimpleUser'));

        SchemaRegistry::getInstance()->clear();
        $this->assertFalse(SchemaRegistry::getInstance()->hasType('SimpleUser'));
    }

    public function testFluentInterface(): void
    {
        $registry = SchemaRegistry::getInstance();
        $result = $registry->register(SimpleUserType::class);

        $this->assertSame($registry, $result);
    }
}

// Test fixtures
#[GraphQLType(name: 'SimpleUser')]
final class SimpleUserType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $email;
}

#[GraphQLType(name: 'SimpleProduct')]
final class SimpleProductType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $title;
}

final class UnattributedClass
{
    public string $name;
}
