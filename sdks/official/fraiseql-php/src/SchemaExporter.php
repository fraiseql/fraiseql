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

        return [
            'version'   => '2.0.0',
            'types'     => self::buildTypes($registry),
            'queries'   => self::buildQueries($registry),
            'mutations' => self::buildMutations($registry),
        ];
    }

    /**
     * Export the schema with federation metadata.
     *
     * Wraps the base schema with a `federation` block that declares this subgraph
     * and lists entity types with their key fields. Error types are excluded from
     * the entity list.
     *
     * @param string $serviceName Logical subgraph name
     * @param string[] $defaultKeyFields Default key fields for types without explicit keyFields
     * @param bool $pretty Pretty-print the JSON
     * @return string JSON string
     */
    public static function exportWithFederation(
        string $serviceName,
        array $defaultKeyFields = ['id'],
        bool $pretty = true,
    ): string {
        $schema = self::toArray();

        $entities = [];
        foreach ($schema['types'] as $type) {
            if (!empty($type['is_error'])) {
                continue;
            }

            $keyFields = $type['key_fields'] ?? $defaultKeyFields;
            $entity = [
                'name'       => $type['name'],
                'key_fields' => $keyFields,
            ];

            if (!empty($type['extends'])) {
                $entity['extends'] = true;
            }

            $entities[] = $entity;
        }

        $schema['federation'] = [
            'enabled'        => true,
            'service_name'   => $serviceName,
            'apollo_version' => 2,
            'entities'       => $entities,
        ];

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
     * Export the schema with federation metadata to a file.
     *
     * @param string $outputPath Destination file path (typically schema.json)
     * @param string $serviceName Logical subgraph name
     * @param string[] $defaultKeyFields Default key fields for types without explicit keyFields
     * @return void
     */
    public static function exportToFileWithFederation(
        string $outputPath,
        string $serviceName,
        array $defaultKeyFields = ['id'],
    ): void {
        $json = self::exportWithFederation($serviceName, $defaultKeyFields, pretty: true);

        if (file_put_contents($outputPath, $json) === false) {
            throw new FraiseQLException("Failed to write schema to: $outputPath");
        }
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
                    static fn (FieldDefinition $f) => [
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

                if ($typeAttr->tenantScoped) {
                    $typeDef['tenant_scoped'] = true;
                }

                if ($typeAttr->keyFields !== null) {
                    $typeDef['key_fields'] = $typeAttr->keyFields;
                }

                if ($typeAttr->extends) {
                    $typeDef['extends'] = true;
                }
            }

            // Also check registry-level tenant_scoped flag (for builder-registered types)
            if (!isset($typeDef['tenant_scoped']) && $registry->isTenantScoped($typeName)) {
                $typeDef['tenant_scoped'] = true;
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
        $merged = array_merge($registry->getInjectDefaults(), $registry->getInjectDefaultsQueries());

        $queries = [];
        foreach ($registry->getAllQueries() as $builder) {
            $query = $builder->toIntermediateArray();
            $query = self::mergeInjectDefaults($query, $merged);
            $queries[] = $query;
        }
        return $queries;
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    private static function buildMutations(SchemaRegistry $registry): array
    {
        $merged = array_merge($registry->getInjectDefaults(), $registry->getInjectDefaultsMutations());

        $mutations = [];
        foreach ($registry->getAllMutations() as $builder) {
            $mutation = $builder->toIntermediateArray();
            $mutation = self::mergeInjectDefaults($mutation, $merged);
            $mutations[] = $mutation;
        }
        return $mutations;
    }

    /**
     * Merge inject defaults into an operation array.
     *
     * For each param in defaults NOT already present in the operation's inject_params,
     * parse "jwt:claim" into {"source":"jwt","claim":"claim"} and add it.
     *
     * @param array<string, mixed> $operation The operation array
     * @param array<string, string> $defaults The inject defaults to merge
     * @return array<string, mixed> The operation with merged inject_params
     */
    private static function mergeInjectDefaults(array $operation, array $defaults): array
    {
        if (empty($defaults)) {
            return $operation;
        }

        $existing = $operation['inject_params'] ?? [];

        foreach ($defaults as $param => $source) {
            if (isset($existing[$param])) {
                continue;
            }
            if (str_starts_with($source, 'jwt:')) {
                $claim = substr($source, 4);
                $existing[$param] = ['source' => 'jwt', 'claim' => $claim];
            }
        }

        if (!empty($existing)) {
            $operation['inject_params'] = $existing;
        }

        return $operation;
    }
}
