<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\JsonSchema;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use FraiseQL\TypeBuilder;
use FraiseQL\FraiseQLException;

/**
 * Tests for JsonSchema.
 *
 * **Read this before adding a fixture.** Every case in this file used to hand-build its
 * document in the shape `SchemaFormatter` emitted — types as a map keyed by name, a
 * `scalars` map, `version` 1.0. `fraiseql compile` refuses that document, and #1245
 * deleted the only thing that produced it. The suite stayed green anyway, because the
 * fixtures agreed with the code under test and neither agreed with the exporter (#1264).
 *
 * So the fixtures here now come from `SchemaExporter` wherever the case allows it, and
 * are written in the exporter's shape — lists of objects carrying their own `name` —
 * where a literal is clearer. A fixture no producer can emit is the defect this file is
 * the record of.
 */
final class JsonSchemaTest extends TestCase
{
    protected function setUp(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    /**
     * A document from the real producer: register, export, parse.
     */
    private static function exported(): JsonSchema
    {
        TypeBuilder::type('User')
            ->scalarField('id', 'ID')
            ->scalarField('name', 'String')
            ->sqlSource('v_user')
            ->register();
        TypeBuilder::type('Post')
            ->scalarField('id', 'ID')
            ->sqlSource('v_post')
            ->register();
        StaticAPI::query('users')->returnType('User')->returnsList(true)->sqlSource('v_user')->register();

        return JsonSchema::fromJson(SchemaExporter::export());
    }

    public function testFromJsonAcceptsWhatTheExporterProduces(): void
    {
        $schema = self::exported();

        $this->assertSame('2.0.0', $schema->version);
        $this->assertSame(['User', 'Post'], $schema->getTypeNames());
        $this->assertSame(2, $schema->getTypeCount());
        $this->assertSame(['users'], $schema->getQueryNames());
        $this->assertSame([], $schema->getMutationNames());
    }

    public function testFromArrayIsTheSameDocumentWithoutTheStringHop(): void
    {
        TypeBuilder::type('User')->scalarField('id', 'ID')->sqlSource('v_user')->register();

        $viaArray = JsonSchema::fromArray(SchemaExporter::toArray());
        $viaJson = JsonSchema::fromJson(SchemaExporter::export());

        $this->assertSame($viaJson->toArray(), $viaArray->toArray());
    }

    /**
     * The case the whole rewrite exists for.
     *
     * Before #1264 this round trip dropped `queries` and `mutations` and invented an
     * empty `scalars`, and `fraiseql compile` then refused the result with
     * `unknown field 'scalars'`. Comparing decoded documents rather than bytes so the
     * case is about content, not about json_encode's whitespace flags.
     */
    public function testTheDocumentSurvivesAFullRoundTripUnchanged(): void
    {
        TypeBuilder::type('User')->scalarField('id', 'ID')->sqlSource('v_user')->register();
        StaticAPI::query('users')->returnType('User')->returnsList(true)->sqlSource('v_user')->register();
        StaticAPI::mutation('createUser')->returnType('User')->sqlSource('fn_create_user')->register();

        $exported = SchemaExporter::export();
        $schema = JsonSchema::fromJson($exported);
        $roundTripped = $schema->toJson();

        // Both accessors, because both reach a file: `toJson()`/`saveToFile()` write one,
        // and a caller handing `toArray()` to its own encoder writes the other.
        $this->assertSame(json_decode($exported, true), $schema->toArray());

        $this->assertSame(
            json_decode($exported, true),
            json_decode($roundTripped, true),
            'A round trip through JsonSchema must not add, drop or reshape a key: the '
            . "compiler's IntermediateSchema denies unknown fields, so an invented key "
            . 'fails the compile and a dropped one silently shrinks the schema.',
        );

        $keys = array_keys(json_decode($roundTripped, true));
        $this->assertContains('queries', $keys);
        $this->assertContains('mutations', $keys);
        $this->assertNotContains('scalars', $keys);
    }

    public function testGetTypeReturnsTheTypeByNameNotByListIndex(): void
    {
        $schema = self::exported();

        $user = $schema->getType('User');
        $this->assertNotNull($user);
        $this->assertSame('User', $user['name']);
        $this->assertSame('v_user', $user['sql_source']);

        $this->assertTrue($schema->hasType('Post'));
        $this->assertFalse($schema->hasType('Missing'));
        $this->assertNull($schema->getType('Missing'));
    }

    public function testAMapOfTypesIsRefusedRatherThanCarriedToTheCompiler(): void
    {
        $this->expectException(FraiseQLException::class);
        $this->expectExceptionMessage("key 'types' must be a JSON array of objects");

        // The pre-2.0.0 shape. Accepted, it would survive as far as saveToFile() and fail
        // in the compiler, one layer away from the mistake that produced it.
        JsonSchema::fromArray([
            'version' => '2.0.0',
            'types' => ['User' => ['name' => 'User', 'fields' => []]],
        ]);
    }

    public function testAMapOfQueriesIsRefusedToo(): void
    {
        $this->expectException(FraiseQLException::class);
        $this->expectExceptionMessage("key 'queries' must be a JSON array of objects");

        JsonSchema::fromArray([
            'version' => '2.0.0',
            'types' => [],
            'queries' => ['users' => ['name' => 'users']],
        ]);
    }

    public function testAnAbsentCollectionIsEmptyNotAnError(): void
    {
        $schema = JsonSchema::fromArray(['version' => '2.0.0']);

        $this->assertSame([], $schema->getTypeNames());
        $this->assertSame([], $schema->getQueryNames());
        $this->assertSame([], $schema->getMutationNames());
        $this->assertSame(0, $schema->getTypeCount());
        $this->assertSame(['version' => '2.0.0'], $schema->toArray());
    }

    public function testVersionDefaultsToTheCurrentFormat(): void
    {
        $this->assertSame('2.0.0', JsonSchema::fromArray(['types' => []])->version);
    }

    public function testInvalidJsonThrows(): void
    {
        $this->expectException(FraiseQLException::class);
        JsonSchema::fromJson('{ invalid json }');
    }

    public function testATopLevelJsonArrayIsNotASchemaDocument(): void
    {
        $this->expectException(FraiseQLException::class);
        $this->expectExceptionMessage('must decode to a JSON object');
        JsonSchema::fromJson('[1, 2, 3]');
    }

    public function testSaveAndLoadFromFile(): void
    {
        $tmpFile = tempnam(sys_get_temp_dir(), 'fraiseql_test_');

        try {
            $schema = self::exported();
            $bytes = $schema->saveToFile($tmpFile);
            $this->assertGreaterThan(0, $bytes);

            $loaded = JsonSchema::loadFromFile($tmpFile);
            $this->assertSame($schema->toArray(), $loaded->toArray());
        } finally {
            if (file_exists($tmpFile)) {
                unlink($tmpFile);
            }
        }
    }

    public function testLoadFromMissingFileThrows(): void
    {
        $this->expectException(FraiseQLException::class);
        $this->expectExceptionMessage('Schema file not found');
        JsonSchema::loadFromFile('/nonexistent/schema.json');
    }
}
