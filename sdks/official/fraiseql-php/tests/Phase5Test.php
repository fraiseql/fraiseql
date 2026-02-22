<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\ArgumentBuilder;
use FraiseQL\ArgumentDefinition;
use FraiseQL\Validator;
use FraiseQL\Cache;
use FraiseQL\CacheKey;
use FraiseQL\SchemaRegistry;
use FraiseQL\TypeBuilder;
use FraiseQL\SchemaFormatter;
use FraiseQL\JsonSchema;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;

/**
 * Tests for Phase 5: Advanced Features
 * - ArgumentBuilder for GraphQL arguments
 * - Validator for schema validation
 * - Cache system for performance
 */
final class Phase5Test extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    // ============ ArgumentBuilder Tests ============

    public function testArgumentBuilderBasic(): void
    {
        $args = ArgumentBuilder::new()
            ->requiredArgument('id', 'Int', 'User ID');

        $this->assertTrue($args->hasArgument('id'));
        $this->assertCount(1, $args->getArguments());

        $arg = $args->getArgument('id');
        $this->assertSame('id', $arg->name);
        $this->assertSame('Int', $arg->type);
        $this->assertFalse($arg->nullable);
    }

    public function testArgumentBuilderMultiple(): void
    {
        $args = ArgumentBuilder::new()
            ->requiredArgument('id', 'Int')
            ->optionalArgument('name', 'String')
            ->optionalArgument('limit', 'Int', defaultValue: 10);

        $this->assertCount(3, $args->getArguments());
        $this->assertTrue($args->hasArgument('id'));
        $this->assertTrue($args->hasArgument('name'));
        $this->assertTrue($args->hasArgument('limit'));
    }

    public function testArgumentBuilderListTypes(): void
    {
        $args = ArgumentBuilder::new()
            ->listArgument('ids', 'Int', 'List of IDs')
            ->optionalListArgument('tags', 'String', 'Optional tag list');

        $idArg = $args->getArgument('ids');
        $this->assertTrue($idArg->isList);
        $this->assertFalse($idArg->nullable);
        $this->assertSame('[Int!]', $idArg->getGraphQLTypeString());

        $tagArg = $args->getArgument('tags');
        $this->assertTrue($tagArg->isList);
        $this->assertTrue($tagArg->nullable);
        $this->assertSame('[String]', $tagArg->getGraphQLTypeString());
    }

    public function testArgumentBuilderObjectTypes(): void
    {
        $args = ArgumentBuilder::new()
            ->objectArgument('user', 'User', 'User object')
            ->optionalObjectArgument('filter', 'FilterInput', 'Filter options');

        $userArg = $args->getArgument('user');
        $this->assertSame('User', $userArg->type);
        $this->assertFalse($userArg->isScalar());

        $filterArg = $args->getArgument('filter');
        $this->assertSame('FilterInput', $filterArg->type);
        $this->assertTrue($filterArg->nullable);
    }

    public function testArgumentBuilderToArray(): void
    {
        $args = ArgumentBuilder::new()
            ->requiredArgument('id', 'Int', 'User ID')
            ->optionalArgument('name', 'String', defaultValue: 'Unknown');

        $array = $args->toArray();
        $this->assertArrayHasKey('id', $array);
        $this->assertArrayHasKey('name', $array);
        $this->assertSame('Int!', $array['id']['type']);
        $this->assertSame('String', $array['name']['type']);
        $this->assertSame('Unknown', $array['name']['defaultValue']);
    }

    public function testArgumentDefinitionToString(): void
    {
        $arg = new ArgumentDefinition(
            name: 'id',
            type: 'Int',
            nullable: false,
        );

        $this->assertSame('id: Int!', (string)$arg);
    }

    public function testArgumentDefinitionIsScalar(): void
    {
        $intArg = new ArgumentDefinition('limit', 'Int');
        $this->assertTrue($intArg->isScalar());

        $userArg = new ArgumentDefinition('user', 'User');
        $this->assertFalse($userArg->isScalar());
    }

    // ============ Validator Tests ============

    public function testValidatorEmptyRegistry(): void
    {
        $registry = SchemaRegistry::getInstance();
        $validator = new Validator();

        $result = $validator->validateRegistry($registry);
        $this->assertTrue($result);
        $this->assertTrue($validator->hasWarnings());
        $this->assertCount(1, $validator->getWarnings());
    }

    public function testValidatorValidRegistry(): void
    {
        SchemaRegistry::getInstance()->register(ValidUserType::class);
        SchemaRegistry::getInstance()->register(ValidPostType::class);

        $validator = new Validator();
        $result = $validator->validateRegistry(SchemaRegistry::getInstance());

        $this->assertTrue($result);
        $this->assertFalse($validator->hasErrors());
    }

    public function testValidatorInvalidTypeName(): void
    {
        $validator = new Validator();

        $result = $validator->validateType(SchemaRegistry::getInstance(), '123Invalid');
        $this->assertFalse($result);
        $this->assertTrue($validator->hasErrors());
    }

    public function testValidatorJsonSchema(): void
    {
        $builder = TypeBuilder::type('Test')
            ->scalarField('id', 'Int');

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatBuilder($builder);

        $validator = new Validator();
        $result = $validator->validateJsonSchema($schema);

        $this->assertTrue($result);
        $this->assertFalse($validator->hasErrors());
    }

    public function testValidatorBuilder(): void
    {
        $builder = TypeBuilder::type('User')
            ->scalarField('id', 'Int')
            ->scalarField('name', 'String');

        $validator = new Validator();
        $result = $validator->validateBuilder($builder);

        $this->assertTrue($result);
        $this->assertFalse($validator->hasErrors());
    }

    public function testValidatorInvalidFieldName(): void
    {
        $validator = new Validator();

        // Create a mock field with invalid name
        $invalidField = new \FraiseQL\FieldDefinition(
            name: '123invalid',
            type: 'String',
            parentType: 'Test',
        );

        $result = $validator->validateField($invalidField, SchemaRegistry::getInstance());
        $this->assertFalse($result);
    }

    public function testValidatorVersion(): void
    {
        $validator = new Validator();

        // Valid versions
        $schema1 = new JsonSchema('1.0', [], []);
        $this->assertTrue($validator->validateJsonSchema($schema1));

        $schema2 = new JsonSchema('2.1.3', [], []);
        $this->assertTrue($validator->validateJsonSchema($schema2));
    }

    public function testValidatorReport(): void
    {
        $validator = new Validator();

        // Validate empty registry to get warnings
        $validator->validateRegistry(SchemaRegistry::getInstance());

        $report = $validator->getReport();
        $this->assertIsString($report);
        $this->assertStringContainsString('WARNINGS', $report);
    }

    // ============ Cache Tests ============

    public function testCacheBasicOperations(): void
    {
        $cache = new Cache();

        $cache->set('key1', 'value1');
        $this->assertTrue($cache->has('key1'));
        $this->assertSame('value1', $cache->get('key1'));

        $cache->delete('key1');
        $this->assertFalse($cache->has('key1'));
        $this->assertNull($cache->get('key1'));
    }

    public function testCacheMultipleEntries(): void
    {
        $cache = new Cache();

        for ($i = 0; $i < 10; $i++) {
            $cache->set("key_$i", "value_$i");
        }

        $this->assertSame(10, $cache->count());

        for ($i = 0; $i < 10; $i++) {
            $this->assertTrue($cache->has("key_$i"));
            $this->assertSame("value_$i", $cache->get("key_$i"));
        }
    }

    public function testCacheClear(): void
    {
        $cache = new Cache();
        $cache->set('key1', 'value1');
        $cache->set('key2', 'value2');

        $this->assertSame(2, $cache->count());

        $cache->clear();
        $this->assertSame(0, $cache->count());
        $this->assertFalse($cache->has('key1'));
    }

    public function testCacheKeys(): void
    {
        $cache = new Cache();
        $cache->set('alpha', 'a');
        $cache->set('beta', 'b');
        $cache->set('gamma', 'c');

        $keys = $cache->keys();
        $this->assertCount(3, $keys);
        $this->assertContains('alpha', $keys);
        $this->assertContains('beta', $keys);
        $this->assertContains('gamma', $keys);
    }

    public function testCacheStats(): void
    {
        $cache = new Cache();
        $cache->setMaxEntries(100);
        $cache->set('key1', 'value1');

        $stats = $cache->getStats();
        $this->assertSame(1, $stats['entries']);
        $this->assertSame(100, $stats['max_entries']);
        $this->assertSame(1, $stats['usage_percent']);
    }

    // ============ CacheKey Tests ============

    public function testCacheKeyForRegistry(): void
    {
        SchemaRegistry::getInstance()->register(ValidUserType::class);

        $key = CacheKey::forRegistry(SchemaRegistry::getInstance());
        $this->assertIsString($key);
        $this->assertStringStartsWith('fraiseql_', $key);

        // Keys should be deterministic
        $key2 = CacheKey::forRegistry(SchemaRegistry::getInstance());
        $this->assertSame($key, $key2);
    }

    public function testCacheKeyForJsonSchema(): void
    {
        $schema = new JsonSchema('1.0', ['Test' => []], []);
        $key = CacheKey::forJsonSchema($schema);

        $this->assertIsString($key);
        $this->assertStringStartsWith('fraiseql_', $key);
    }

    public function testCacheKeyForBuilder(): void
    {
        $builder = TypeBuilder::type('User')
            ->scalarField('id', 'Int');

        $key = CacheKey::forBuilder($builder);
        $this->assertIsString($key);
        $this->assertStringStartsWith('fraiseql_', $key);

        // Same builder should have same key
        $key2 = CacheKey::forBuilder($builder);
        $this->assertSame($key, $key2);
    }

    public function testCacheKeyForType(): void
    {
        $key = CacheKey::forType('User');
        $this->assertIsString($key);
        $this->assertStringStartsWith('fraiseql_', $key);

        // Same type name should have same key
        $key2 = CacheKey::forType('User');
        $this->assertSame($key, $key2);
    }

    public function testCacheKeyForField(): void
    {
        $key = CacheKey::forField('User', 'id');
        $this->assertIsString($key);
        $this->assertStringStartsWith('fraiseql_', $key);

        // Different field should have different key
        $key2 = CacheKey::forField('User', 'name');
        $this->assertNotSame($key, $key2);
    }

    public function testCacheKeyCustom(): void
    {
        $key = CacheKey::custom('custom_namespace', ['action' => 'compile', 'version' => '1.0']);
        $this->assertIsString($key);
        $this->assertStringStartsWith('fraiseql_', $key);
    }

    // ============ Integration Tests ============

    public function testValidatorWithCache(): void
    {
        $cache = new Cache();

        // Validate and cache result
        SchemaRegistry::getInstance()->register(ValidUserType::class);
        $key = CacheKey::forRegistry(SchemaRegistry::getInstance());

        $validator = new Validator();
        $result = $validator->validateRegistry(SchemaRegistry::getInstance());

        $cache->set($key, $result);
        $this->assertTrue($cache->has($key));
        $this->assertTrue($cache->get($key));
    }

    public function testArgumentsWithSchema(): void
    {
        // Build a field with arguments
        $args = ArgumentBuilder::new()
            ->requiredArgument('id', 'Int')
            ->optionalArgument('filter', 'String');

        $builder = TypeBuilder::type('Query')
            ->field('user', 'User');

        $this->assertTrue($builder->hasField('user'));
        $this->assertCount(1, $args->getArguments());
    }

    public function testComplexValidation(): void
    {
        SchemaRegistry::getInstance()->register(ValidUserType::class);
        SchemaRegistry::getInstance()->register(ValidPostType::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        $validator = new Validator();
        $result = $validator->validateJsonSchema($schema);

        $this->assertTrue($result);
        $this->assertFalse($validator->hasErrors());
        $this->assertCount(2, $schema->getTypeNames());
    }
}

// Test fixtures
#[GraphQLType(name: 'ValidUser')]
final class ValidUserType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;
}

#[GraphQLType(name: 'ValidPost')]
final class ValidPostType
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $title;

    #[GraphQLField(type: 'ValidUser')]
    public ValidUserType $author;
}
