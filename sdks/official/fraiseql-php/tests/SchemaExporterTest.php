<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Schema;
use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

// ---------------------------------------------------------------------------
// Fixture classes
// ---------------------------------------------------------------------------

#[GraphQLType(name: 'Author', sqlSource: 'v_author', description: 'A blog author')]
final class SchemaExporterAuthor
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $bio;

    #[GraphQLField(type: 'DateTime', nullable: false)]
    public string $createdAt;
}

#[GraphQLType(name: 'Post', sqlSource: 'v_post')]
final class SchemaExporterPost
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $title;

    #[GraphQLField(type: 'ID', nullable: false)]
    public int $authorId;
}

/**
 * Golden tests for SchemaExporter — verifies output exactly matches the
 * IntermediateSchema format consumed by `fraiseql compile`.
 */
class SchemaExporterTest extends TestCase
{
    protected function setUp(): void
    {
        Schema::reset();
    }

    protected function tearDown(): void
    {
        Schema::reset();
    }

    // -------------------------------------------------------------------------
    // Structure tests
    // -------------------------------------------------------------------------

    public function testExportHasRequiredTopLevelKeys(): void
    {
        $schema = SchemaExporter::toArray();

        $this->assertArrayHasKey('version', $schema);
        $this->assertArrayHasKey('types', $schema);
        $this->assertArrayHasKey('queries', $schema);
        $this->assertArrayHasKey('mutations', $schema);
    }

    public function testVersionIs200(): void
    {
        $schema = SchemaExporter::toArray();
        $this->assertSame('2.0.0', $schema['version']);
    }

    public function testEmptySchemaHasEmptyArrays(): void
    {
        $schema = SchemaExporter::toArray();

        $this->assertSame([], $schema['types']);
        $this->assertSame([], $schema['queries']);
        $this->assertSame([], $schema['mutations']);
    }

    // -------------------------------------------------------------------------
    // Types
    // -------------------------------------------------------------------------

    public function testTypeExportedAsArrayNotMap(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();

        // Must be a sequential list, not a string-keyed map
        $this->assertSame([0], array_keys($schema['types']));
    }

    public function testTypeHasCorrectName(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();
        $this->assertSame('Author', $schema['types'][0]['name']);
    }

    public function testTypeHasSqlSource(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();
        $this->assertSame('v_author', $schema['types'][0]['sql_source']);
    }

    public function testTypeHasDescription(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();
        $this->assertSame('A blog author', $schema['types'][0]['description']);
    }

    public function testTypeFieldsAreArray(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();
        $fields = $schema['types'][0]['fields'];

        $this->assertIsArray($fields);
        $this->assertNotEmpty($fields);
        // Sequential (not string-keyed map)
        $this->assertSame(range(0, count($fields) - 1), array_keys($fields));
    }

    public function testTypeFieldHasNameTypeNullable(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();
        $fields = $schema['types'][0]['fields'];

        foreach ($fields as $field) {
            $this->assertArrayHasKey('name', $field);
            $this->assertArrayHasKey('type', $field);
            $this->assertArrayHasKey('nullable', $field);
            $this->assertIsBool($field['nullable']);
        }
    }

    public function testNullableFieldCorrectlyMarked(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $schema = SchemaExporter::toArray();
        $fields = $schema['types'][0]['fields'];

        $byName = [];
        foreach ($fields as $f) {
            $byName[$f['name']] = $f;
        }

        $this->assertFalse($byName['id']['nullable']);
        $this->assertFalse($byName['name']['nullable']);
        $this->assertTrue($byName['bio']['nullable']);
    }

    public function testMultipleTypesExported(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);
        StaticAPI::register(SchemaExporterPost::class);

        $schema    = SchemaExporter::toArray();
        $typeNames = array_column($schema['types'], 'name');

        $this->assertCount(2, $schema['types']);
        $this->assertContains('Author', $typeNames);
        $this->assertContains('Post', $typeNames);
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    public function testQueryExportedAsArrayNotMap(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();

        $schema = SchemaExporter::toArray();

        $this->assertSame([0], array_keys($schema['queries']));
    }

    public function testQueryHasReturnTypeSnakeCase(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();

        $query = SchemaExporter::toArray()['queries'][0];

        // Must use snake_case 'return_type', NOT camelCase 'returnType'
        $this->assertArrayHasKey('return_type', $query);
        $this->assertArrayNotHasKey('returnType', $query);
        $this->assertSame('Author', $query['return_type']);
    }

    public function testQueryReturnsList(): void
    {
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();

        StaticAPI::query('author')
            ->returnType('Author')
            ->returnsList(false)
            ->sqlSource('v_author')
            ->argument('id', 'ID', false)
            ->register();

        $queries = SchemaExporter::toArray()['queries'];
        $byName  = [];
        foreach ($queries as $q) {
            $byName[$q['name']] = $q;
        }

        $this->assertTrue($byName['authors']['returns_list']);
        $this->assertFalse($byName['author']['returns_list']);
    }

    public function testQueryHasSqlSource(): void
    {
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();

        $query = SchemaExporter::toArray()['queries'][0];
        $this->assertSame('v_author', $query['sql_source']);
    }

    public function testQueryArgumentsAreArray(): void
    {
        StaticAPI::query('author')
            ->returnType('Author')
            ->sqlSource('v_author')
            ->argument('id', 'ID', false)
            ->register();

        $query = SchemaExporter::toArray()['queries'][0];

        $this->assertIsArray($query['arguments']);
        $this->assertCount(1, $query['arguments']);

        $arg = $query['arguments'][0];
        $this->assertSame('id', $arg['name']);
        $this->assertSame('ID', $arg['type']);
        $this->assertFalse($arg['nullable']);
    }

    public function testQueryWithNoArgumentsHasEmptyArray(): void
    {
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();

        $query = SchemaExporter::toArray()['queries'][0];
        $this->assertSame([], $query['arguments']);
    }

    public function testQueryCacheTtlSeconds(): void
    {
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->cacheTtlSeconds(300)
            ->register();

        $query = SchemaExporter::toArray()['queries'][0];
        $this->assertSame(300, $query['cache_ttl_seconds']);
    }

    // -------------------------------------------------------------------------
    // Mutations
    // -------------------------------------------------------------------------

    public function testMutationExportedAsArray(): void
    {
        StaticAPI::mutation('createAuthor')
            ->returnType('Author')
            ->sqlSource('fn_create_author')
            ->operation('insert')
            ->register();

        $schema = SchemaExporter::toArray();

        $this->assertSame([0], array_keys($schema['mutations']));
    }

    public function testMutationHasReturnTypeSnakeCase(): void
    {
        StaticAPI::mutation('createAuthor')
            ->returnType('Author')
            ->sqlSource('fn_create_author')
            ->register();

        $mutation = SchemaExporter::toArray()['mutations'][0];

        $this->assertArrayHasKey('return_type', $mutation);
        $this->assertArrayNotHasKey('returnType', $mutation);
        $this->assertSame('Author', $mutation['return_type']);
    }

    public function testMutationHasSqlSourceAndOperation(): void
    {
        StaticAPI::mutation('createAuthor')
            ->returnType('Author')
            ->sqlSource('fn_create_author')
            ->operation('insert')
            ->register();

        $mutation = SchemaExporter::toArray()['mutations'][0];
        $this->assertSame('fn_create_author', $mutation['sql_source']);
        $this->assertSame('insert', $mutation['operation']);
    }

    // -------------------------------------------------------------------------
    // JSON output
    // -------------------------------------------------------------------------

    public function testExportReturnsValidJson(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();

        $json = SchemaExporter::export();
        $this->assertJson($json);
    }

    public function testExportPrettyContainsNewlines(): void
    {
        $json = SchemaExporter::export(pretty: true);
        $this->assertStringContainsString("\n", $json);
    }

    public function testExportCompactNoNewlines(): void
    {
        $json = SchemaExporter::export(pretty: false);
        $this->assertStringNotContainsString("\n", $json);
    }

    public function testExportToFile(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);

        $tmpFile = tempnam(sys_get_temp_dir(), 'fraiseql_schema_') . '.json';

        try {
            SchemaExporter::exportToFile($tmpFile);

            $this->assertFileExists($tmpFile);
            $content = (string) file_get_contents($tmpFile);
            $schema  = json_decode($content, true);

            $this->assertSame('2.0.0', $schema['version']);
            $this->assertCount(1, $schema['types']);
        } finally {
            if (file_exists($tmpFile)) {
                unlink($tmpFile);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Golden test — exact shape
    // -------------------------------------------------------------------------

    public function testGoldenSchema(): void
    {
        StaticAPI::register(SchemaExporterAuthor::class);
        StaticAPI::query('authors')
            ->returnType('Author')
            ->returnsList(true)
            ->sqlSource('v_author')
            ->register();
        StaticAPI::query('author')
            ->returnType('Author')
            ->returnsList(false)
            ->sqlSource('v_author')
            ->argument('id', 'ID', false)
            ->register();
        StaticAPI::mutation('createAuthor')
            ->returnType('Author')
            ->sqlSource('fn_create_author')
            ->operation('insert')
            ->argument('name', 'String', false)
            ->register();

        $schema = SchemaExporter::toArray();

        // Top-level structure
        $this->assertSame('2.0.0', $schema['version']);

        // Type: Author
        $author = $schema['types'][0];
        $this->assertSame('Author', $author['name']);
        $this->assertSame('v_author', $author['sql_source']);
        $this->assertSame('A blog author', $author['description']);

        // Fields are a sequential array
        $fieldNames = array_column($author['fields'], 'name');
        $this->assertContains('id', $fieldNames);
        $this->assertContains('name', $fieldNames);
        $this->assertContains('bio', $fieldNames);

        // Queries
        $queryNames = array_column($schema['queries'], 'name');
        $this->assertContains('authors', $queryNames);
        $this->assertContains('author', $queryNames);

        $authors = null;
        $author2 = null;
        foreach ($schema['queries'] as $q) {
            if ($q['name'] === 'authors') {
                $authors = $q;
            }
            if ($q['name'] === 'author') {
                $author2 = $q;
            }
        }

        $this->assertNotNull($authors);
        $this->assertSame('Author', $authors['return_type']);
        $this->assertTrue($authors['returns_list']);
        $this->assertSame([], $authors['arguments']);

        $this->assertNotNull($author2);
        $this->assertFalse($author2['returns_list']);
        $this->assertCount(1, $author2['arguments']);
        $this->assertSame('id', $author2['arguments'][0]['name']);
        $this->assertSame('ID', $author2['arguments'][0]['type']);
        $this->assertFalse($author2['arguments'][0]['nullable']);

        // Mutation
        $mutation = $schema['mutations'][0];
        $this->assertSame('createAuthor', $mutation['name']);
        $this->assertSame('Author', $mutation['return_type']);
        $this->assertSame('fn_create_author', $mutation['sql_source']);
        $this->assertSame('insert', $mutation['operation']);
        $this->assertCount(1, $mutation['arguments']);
    }
}
