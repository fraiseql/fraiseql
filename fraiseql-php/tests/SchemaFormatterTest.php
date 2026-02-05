<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaFormatter;
use FraiseQL\SchemaRegistry;
use FraiseQL\TypeBuilder;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;

/**
 * Tests for SchemaFormatter class.
 */
final class SchemaFormatterTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testFormatRegistry(): void
    {
        SchemaRegistry::getInstance()->register(FormatterUserType::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        $this->assertSame('1.0', $schema->version);
        $this->assertTrue($schema->hasType('FormatterUser'));
    }

    public function testFormatRegistryWithDescription(): void
    {
        SchemaRegistry::getInstance()->register(FormatterUserType::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(
            SchemaRegistry::getInstance(),
            description: 'User management schema',
        );

        $this->assertSame('User management schema', $schema->description);
    }

    public function testFormatBuilderSingleType(): void
    {
        $builder = TypeBuilder::type('Query')
            ->field('hello', 'String')
            ->field('users', 'User', isList: true);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $this->assertTrue($schema->hasType('Query'));
        $this->assertCount(2, $schema->getType('Query')['fields']);
    }

    public function testFormatBuilders(): void
    {
        $userBuilder = TypeBuilder::type('User')
            ->scalarField('id', 'Int')
            ->scalarField('name', 'String');

        $queryBuilder = TypeBuilder::type('Query')
            ->field('user', 'User');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilders($userBuilder, $queryBuilder);

        $this->assertTrue($schema->hasType('User'));
        $this->assertTrue($schema->hasType('Query'));
        $this->assertSame(2, $schema->getTypeCount());
    }

    public function testFormatFieldWithDescription(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('email', 'String', description: 'User email address');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $type = $schema->getType('User');
        $this->assertSame('User email address', $type['fields']['email']['description']);
    }

    public function testFormatFieldWithResolver(): void
    {
        $builder = TypeBuilder::type('User')
            ->field('fullName', 'String')
            ->withResolver('fullName', 'getFullName');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $type = $schema->getType('User');
        $this->assertSame('getFullName', $type['fields']['fullName']['resolver']);
    }

    public function testFormatFieldGraphQLType(): void
    {
        $builder = TypeBuilder::type('Post')
            ->field('id', 'Int')
            ->optionalField('content', 'String')
            ->listField('tags', 'String');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $type = $schema->getType('Post');
        $this->assertSame('Int!', $type['fields']['id']['type']);
        $this->assertSame('String', $type['fields']['content']['type']);
        $this->assertSame('[String!]', $type['fields']['tags']['type']);
    }

    public function testFormatRegistryTracksScalars(): void
    {
        SchemaRegistry::getInstance()->register(FormatterUserType::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        $this->assertTrue(in_array('Int', $schema->getScalarNames(), true));
        $this->assertTrue(in_array('String', $schema->getScalarNames(), true));
    }

    public function testFormatTypeWithMultipleFields(): void
    {
        $builder = TypeBuilder::type('Product')
            ->scalarField('id', 'Int', 'Product ID')
            ->scalarField('name', 'String', 'Product name')
            ->scalarField('price', 'Float', 'Product price')
            ->optionalField('description', 'String');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $type = $schema->getType('Product');
        $this->assertCount(4, $type['fields']);
    }

    public function testFormatBuilderDescription(): void
    {
        $builder = TypeBuilder::type('User')
            ->scalarField('id', 'Int')
            ->description('A user in the system');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $type = $schema->getType('User');
        $this->assertSame('A user in the system', $type['description']);
    }
}

// Test fixtures
#[GraphQLType(name: 'FormatterUser')]
final class FormatterUserType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $email;
}
