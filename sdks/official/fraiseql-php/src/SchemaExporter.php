<?php

declare(strict_types=1);

namespace FraiseQL;

use FraiseQL\Attributes\GraphQLType;

/**
 * Exports the registered schema in the canonical IntermediateSchema format
 * consumed by `fraiseql compile`.
 *
 * Output format matches the Rust `IntermediateSchema` struct exactly:
 * - `version`: "2.0.0"
 * - `types`: array of type objects (not a map)
 * - `queries`: array of query objects (not a map)
 * - `mutations`: array of mutation objects (not a map)
 * - All keys snake_case
 *
 * Usage:
 * ```php
 * StaticAPI::register(Author::class);
 * StaticAPI::register(Post::class);
 * StaticAPI::query('authors')->returnType('Author')->returnsList()->sqlSource('v_author')->register();
 *
 * $json = SchemaExporter::export();
 * file_put_contents('schema.json', $json);
 * // Then: fraiseql compile schema.json
 * ```
 */
final class SchemaExporter
{
    /**
     * Export the complete schema as a JSON string in IntermediateSchema format.
     *
     * @param bool $pretty Pretty-print the JSON output
     * @return string JSON string
     */
    public static function export(bool $pretty = true): string
    {
        $schema = self::toArray();

        $flags = JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE;
        if ($pretty) {
            $flags |= JSON_PRETTY_PRINT;
        }

        $json = json_encode($schema, $flags);
        if ($json === false) {
            throw new FraiseQLException('Failed to encode schema as JSON: ' . json_last_error_msg());
        }

        return $json;
    }

    /**
     * Export the schema to a file.
     *
     * @param string $outputPath Destination file path (typically schema.json)
     * @return void
     */
    public static function exportToFile(string $outputPath): void
    {
        $json = self::export(pretty: true);

        if (file_put_contents($outputPath, $json) === false) {
            throw new FraiseQLException("Failed to write schema to: $outputPath");
        }
    }

    /**
     * Build the IntermediateSchema array.
     *
     * @return array<string, mixed>
     */
    public static function toArray(): array
    {
        $registry = SchemaRegistry::getInstance();

        $schema = [
            'version'   => '2.0.0',
            'types'     => self::buildTypes($registry),
            'queries'   => self::buildQueries($registry),
            'mutations' => self::buildMutations($registry),
        ];

        $inputTypes = $registry->getAllInputTypes();
        if (!empty($inputTypes)) {
            $schema['input_types'] = array_values($inputTypes);
        }

        return $schema;
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    private static function buildTypes(SchemaRegistry $registry): array
    {
        $types = [];

        foreach ($registry->getTypeNames() as $typeName) {
            /** @var GraphQLType|null $typeAttr */
            $typeAttr = $registry->getType($typeName);
            $fields   = $registry->getTypeFields($typeName);

            $typeDef = [
                'name'   => $typeName,
                'fields' => array_values(array_map(
                    static fn(FieldDefinition $f) => [
                        'name'     => $f->name,
                        'type'     => $f->type,
                        'nullable' => $f->nullable,
                    ],
                    $fields,
                )),
            ];

            if ($typeAttr !== null) {
                if ($typeAttr->sqlSource !== null) {
                    $typeDef['sql_source'] = $typeAttr->sqlSource;
                }

                if ($typeAttr->description !== null) {
                    $typeDef['description'] = $typeAttr->description;
                }

                if ($typeAttr->isInput) {
                    $typeDef['is_input'] = true;
                }

                if ($typeAttr->relay) {
                    $typeDef['relay'] = true;
                }

                if ($typeAttr->isError) {
                    $typeDef['is_error'] = true;
                }
            }

            $types[] = $typeDef;
        }

        return $types;
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    private static function buildQueries(SchemaRegistry $registry): array
    {
        $queries = [];
        foreach ($registry->getAllQueries() as $builder) {
            $queries[] = $builder->toIntermediateArray();
        }
        return $queries;
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    private static function buildMutations(SchemaRegistry $registry): array
    {
        $mutations = [];
        foreach ($registry->getAllMutations() as $builder) {
            $mutations[] = $builder->toIntermediateArray();
        }
        return $mutations;
    }
}
