<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\JsonSchema;
use FraiseQL\FraiseQLException;

/**
 * Tests for JsonSchema class.
 */
final class JsonSchemaTest extends TestCase
{
    public function testCreateJsonSchema(): void
    {
        $types = ['User' => ['name' => 'User', 'fields' => []]];
        $schema = new JsonSchema('1.0', $types, ['Int' => 'Integer']);

        $this->assertSame('1.0', $schema->version);
        $this->assertCount(1, $schema->types);
        $this->assertCount(1, $schema->scalars);
    }

    public function testToArray(): void
    {
        $types = ['User' => ['name' => 'User', 'fields' => ['id' => ['type' => 'Int!']]]];
        $schema = new JsonSchema('1.0', $types, ['Int' => 'Integer']);

        $array = $schema->toArray();

        $this->assertSame('1.0', $array['version']);
        $this->assertArrayHasKey('types', $array);
        $this->assertArrayHasKey('scalars', $array);
    }

    public function testToArrayWithDescription(): void
    {
        $schema = new JsonSchema(
            '1.0',
            [],
            [],
            description: 'Test schema',
        );

        $array = $schema->toArray();
        $this->assertSame('Test schema', $array['description']);
    }

    public function testToArrayWithMetadata(): void
    {
        $metadata = ['author' => 'Test', 'created' => '2026-01-14'];
        $schema = new JsonSchema('1.0', [], [], metadata: $metadata);

        $array = $schema->toArray();
        $this->assertSame($metadata, $array['metadata']);
    }

    public function testToJson(): void
    {
        $types = ['User' => ['name' => 'User', 'fields' => []]];
        $schema = new JsonSchema('1.0', $types, ['Int' => 'Integer']);

        $json = $schema->toJson();

        $this->assertIsString($json);
        $this->assertStringContainsString('version', $json);
        $this->assertStringContainsString('User', $json);
    }

    public function testFromJson(): void
    {
        $json = json_encode([
            'version' => '1.0',
            'types' => ['User' => ['name' => 'User', 'fields' => []]],
            'scalars' => ['Int' => 'Integer'],
            'description' => 'Test',
        ]);

        $schema = JsonSchema::fromJson($json);

        $this->assertSame('1.0', $schema->version);
        $this->assertTrue($schema->hasType('User'));
        $this->assertSame('Test', $schema->description);
    }

    public function testGetTypeNames(): void
    {
        $types = [
            'User' => ['name' => 'User', 'fields' => []],
            'Product' => ['name' => 'Product', 'fields' => []],
        ];
        $schema = new JsonSchema('1.0', $types, []);

        $names = $schema->getTypeNames();
        $this->assertContains('User', $names);
        $this->assertContains('Product', $names);
    }

    public function testGetType(): void
    {
        $types = ['User' => ['name' => 'User', 'fields' => ['id' => ['type' => 'Int!']]]];
        $schema = new JsonSchema('1.0', $types, []);

        $type = $schema->getType('User');
        $this->assertNotNull($type);
        $this->assertSame('User', $type['name']);
    }

    public function testHasType(): void
    {
        $types = ['User' => ['name' => 'User', 'fields' => []]];
        $schema = new JsonSchema('1.0', $types, []);

        $this->assertTrue($schema->hasType('User'));
        $this->assertFalse($schema->hasType('Product'));
    }

    public function testGetTypeCount(): void
    {
        $types = [
            'User' => ['name' => 'User', 'fields' => []],
            'Product' => ['name' => 'Product', 'fields' => []],
        ];
        $schema = new JsonSchema('1.0', $types, []);

        $this->assertSame(2, $schema->getTypeCount());
    }

    public function testGetScalarNames(): void
    {
        $scalars = ['Int' => 'Integer', 'String' => 'String'];
        $schema = new JsonSchema('1.0', [], $scalars);

        $names = $schema->getScalarNames();
        $this->assertContains('Int', $names);
        $this->assertContains('String', $names);
    }

    public function testInvalidJsonThrows(): void
    {
        $this->expectException(FraiseQLException::class);
        JsonSchema::fromJson('{ invalid json }');
    }

    public function testSaveAndLoadFromFile(): void
    {
        $tmpFile = tempnam(sys_get_temp_dir(), 'fraiseql_test_');

        try {
            $types = ['User' => ['name' => 'User', 'fields' => []]];
            $schema = new JsonSchema('1.0', $types, ['Int' => 'Integer']);

            $schema->saveToFile($tmpFile);
            $this->assertFileExists($tmpFile);

            $loaded = JsonSchema::loadFromFile($tmpFile);
            $this->assertSame('1.0', $loaded->version);
            $this->assertTrue($loaded->hasType('User'));
        } finally {
            if (file_exists($tmpFile)) {
                unlink($tmpFile);
            }
        }
    }
}
