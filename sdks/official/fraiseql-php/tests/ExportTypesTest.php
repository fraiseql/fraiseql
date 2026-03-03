<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Schema;
use FraiseQL\SchemaRegistry;
use PHPUnit\Framework\TestCase;

// ---------------------------------------------------------------------------
// Fixture classes (defined once at file scope to avoid redeclaration)
// ---------------------------------------------------------------------------

#[GraphQLType(name: 'ExportUser', sqlSource: 'v_export_user', description: 'A user in the system')]
final class ExportUserFixture
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $name;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $email;
}

#[GraphQLType(name: 'ExportPost', sqlSource: 'v_export_post')]
final class ExportPostFixture
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $title;

    #[GraphQLField(type: 'ID', nullable: false)]
    public int $authorId;
}

/**
 * Tests for the minimal types export (Schema::exportTypes).
 *
 * Schema::exportTypes() produces {"types": [...]} for the TOML-based
 * workflow where queries/mutations live in fraiseql.toml.
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

    public function testExportTypesSingleType(): void
    {
        Schema::registerType(ExportUserFixture::class);

        $json   = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        $this->assertArrayHasKey('types', $parsed);
        $this->assertIsArray($parsed['types']);
        $this->assertCount(1, $parsed['types']);

        // Should NOT have queries/mutations (types-only export)
        $this->assertArrayNotHasKey('queries', $parsed);
        $this->assertArrayNotHasKey('mutations', $parsed);

        $typeDef = $parsed['types'][0];
        $this->assertSame('ExportUser', $typeDef['name']);
        $this->assertSame('A user in the system', $typeDef['description'] ?? null);
    }

    public function testExportTypesMultipleTypes(): void
    {
        Schema::registerType(ExportUserFixture::class);
        Schema::registerType(ExportPostFixture::class);

        $json   = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        $this->assertCount(2, $parsed['types']);

        $typeNames = array_column($parsed['types'], 'name');
        $this->assertContains('ExportUser', $typeNames);
        $this->assertContains('ExportPost', $typeNames);
    }

    public function testExportTypesNoQueries(): void
    {
        Schema::registerType(ExportUserFixture::class);

        $json   = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        $this->assertArrayHasKey('types', $parsed);
        $this->assertArrayNotHasKey('queries', $parsed);
        $this->assertArrayNotHasKey('mutations', $parsed);
    }

    public function testExportTypesCompactVsPretty(): void
    {
        Schema::registerType(ExportUserFixture::class);

        $compact = Schema::exportTypes(false);
        $pretty  = Schema::exportTypes(true);

        // Both are valid JSON
        $this->assertIsArray(json_decode($compact, true));
        $this->assertIsArray(json_decode($pretty, true));

        // Pretty output contains newlines; compact does not
        $this->assertStringContainsString("\n", $pretty);
        $this->assertStringNotContainsString("\n", $compact);
    }

    public function testExportTypesFile(): void
    {
        Schema::registerType(ExportUserFixture::class);

        $tmpFile = tempnam(sys_get_temp_dir(), 'fraiseql_export_') . '.json';

        try {
            ob_start();
            Schema::exportTypesFile($tmpFile);
            ob_end_clean();

            $this->assertFileExists($tmpFile);
            $parsed = json_decode((string) file_get_contents($tmpFile), true);
            $this->assertArrayHasKey('types', $parsed);
            $this->assertCount(1, $parsed['types']);
        } finally {
            if (file_exists($tmpFile)) {
                unlink($tmpFile);
            }
        }
    }

    public function testExportTypesEmpty(): void
    {
        $json   = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        $this->assertArrayHasKey('types', $parsed);
        $this->assertSame([], $parsed['types']);
    }

    public function testExportTypesFieldsAreArray(): void
    {
        Schema::registerType(ExportUserFixture::class);

        $json   = Schema::exportTypes(true);
        $parsed = json_decode($json, true);

        $typeDef = $parsed['types'][0];
        $this->assertIsArray($typeDef['fields']);
        $this->assertNotEmpty($typeDef['fields']);

        // Each field must have name, type, nullable
        foreach ($typeDef['fields'] as $field) {
            $this->assertArrayHasKey('name', $field);
            $this->assertArrayHasKey('type', $field);
            $this->assertArrayHasKey('nullable', $field);
        }
    }
}
