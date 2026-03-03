<?php

namespace FraiseQL\Tests;

use FraiseQL\Schema;
use FraiseQL\TypeInfo;
use FraiseQL\FieldDefinition;
use PHPUnit\Framework\TestCase;

/**
 * Tests for minimal types.json export (TOML-based workflow)
 *
 * Validates that ExportTypes() function generates minimal schema
 * with only types (no queries, mutations, observers, etc.)
 */
class ExportTypesTest extends TestCase
{
    protected function setUp(): void
    {
        Schema::reset();
    }

    protected function tearDown(): void
    {
        Schema::reset();
    }

    public function testExportTypesMinimalSingleType(): void
    {
        // Register a single type with fields
        Schema::registerType('User', [
            'name' => 'User',
            'description' => 'User in the system',
            'fields' => [
                ['name' => 'id', 'type' => 'ID', 'nullable' => false],
                ['name' => 'name', 'type' => 'String', 'nullable' => false],
                ['name' => 'email', 'type' => 'String', 'nullable' => false],
            ]
        ]);

        // Export minimal types
        $json = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        // Should have types section
        $this->assertArrayHasKey('types', $parsed);
        $this->assertIsArray($parsed['types']);
        $this->assertCount(1, $parsed['types']);

        // Should NOT have queries, mutations, observers
        $this->assertArrayNotHasKey('queries', $parsed);
        $this->assertArrayNotHasKey('mutations', $parsed);
        $this->assertArrayNotHasKey('observers', $parsed);
        $this->assertArrayNotHasKey('authz_policies', $parsed);

        // Verify User type
        $userDef = $parsed['types'][0];
        $this->assertEquals('User', $userDef['name']);
        $this->assertEquals('User in the system', $userDef['description']);
    }

    public function testExportTypesMultipleTypes(): void
    {
        // Register User type
        $userType = new TypeInfo('User', '');
        $userType->addField(new FieldDefinition('id', 'ID', false));
        $userType->addField(new FieldDefinition('name', 'String', false));
        Schema::registerType('User', $userType);

        // Register Post type
        $postType = new TypeInfo('Post', '');
        $postType->addField(new FieldDefinition('id', 'ID', false));
        $postType->addField(new FieldDefinition('title', 'String', false));
        $postType->addField(new FieldDefinition('authorId', 'ID', false));
        Schema::registerType('Post', $postType);

        // Export minimal
        $json = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        // Check types count
        $this->assertCount(2, $parsed['types']);

        // Verify both types present
        $typeNames = array_map(function ($t) { return $t['name']; }, $parsed['types']);
        $this->assertContains('User', $typeNames);
        $this->assertContains('Post', $typeNames);
    }

    public function testExportTypesNoQueries(): void
    {
        // Register type
        $userType = new TypeInfo('User', '');
        $userType->addField(new FieldDefinition('id', 'ID', false));
        Schema::registerType('User', $userType);

        // Export minimal
        $json = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        // Should have types
        $this->assertArrayHasKey('types', $parsed);

        // Should NOT have queries
        $this->assertArrayNotHasKey('queries', $parsed);
        $this->assertArrayNotHasKey('mutations', $parsed);
    }

    public function testExportTypesCompactFormat(): void
    {
        // Register type
        $userType = new TypeInfo('User', '');
        $userType->addField(new FieldDefinition('id', 'ID', false));
        Schema::registerType('User', $userType);

        // Export compact (pretty=false)
        $compactJson = Schema::exportTypes(false);
        $prettyJson = Schema::exportTypes(true);

        // Both should be valid JSON
        $this->assertIsArray(json_decode($compactJson, true));
        $this->assertIsArray(json_decode($prettyJson, true));

        // Compact should be smaller
        $this->assertLessThan(strlen($prettyJson), strlen($compactJson) + 100);
    }

    public function testExportTypesPrettyFormat(): void
    {
        // Register type
        $userType = new TypeInfo('User', '');
        $userType->addField(new FieldDefinition('id', 'ID', false));
        Schema::registerType('User', $userType);

        // Export pretty
        $json = Schema::exportTypes(true);

        // Should contain newlines (pretty format)
        $this->assertStringContainsString("\n", $json);

        // Should be valid JSON
        $parsed = json_decode($json, true);
        $this->assertArrayHasKey('types', $parsed);
    }

    public function testExportTypesFile(): void
    {
        // Register type
        $userType = new TypeInfo('User', '');
        $userType->addField(new FieldDefinition('id', 'ID', false));
        $userType->addField(new FieldDefinition('name', 'String', false));
        Schema::registerType('User', $userType);

        // Export to temporary file
        $tmpFile = '/tmp/fraiseql_types_test_php.json';

        // Clean up if exists
        if (file_exists($tmpFile)) {
            unlink($tmpFile);
        }

        // Export to file
        Schema::exportTypesFile($tmpFile);

        // Verify file exists
        $this->assertTrue(file_exists($tmpFile));

        // Verify content
        $content = file_get_contents($tmpFile);
        $parsed = json_decode($content, true);

        $this->assertArrayHasKey('types', $parsed);
        $this->assertCount(1, $parsed['types']);

        // Cleanup
        unlink($tmpFile);
    }

    public function testExportTypesEmpty(): void
    {
        // Export with no types registered
        $json = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        // Should still have types key (as empty array)
        $this->assertArrayHasKey('types', $parsed);
        $this->assertIsArray($parsed['types']);
        $this->assertEmpty($parsed['types']);
    }
}
