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
 * - `subscriptions`: array of subscription objects, when any are registered
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

        $enums = $registry->getAllEnums();
        if (!empty($enums)) {
            $schema['enums'] = array_values($enums);
        }

        // Registered subscriptions used to reach no key at all: `SubscriptionBuilder`
        // registered them, `SchemaRegistry` stored them, and this method never read
        // them back — so a PHP author's subscription was silently absent from the
        // document rather than compiled (#1024).
        $subscriptions = $registry->getAllSubscriptions();
        if (!empty($subscriptions)) {
            $schema['subscriptions'] = array_map(
                fn (SubscriptionDefinition $subscription) => $subscription->toArray(),
                array_values($subscriptions),
            );
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
                    static function (FieldDefinition $f): array {
                        $field = [
                            'name'     => $f->name,
                            'type'     => $f->type,
                            'nullable' => $f->nullable,
                        ];

                        // `IntermediateField` has always carried a description and
                        // `FieldDefinition` has always held one; this map simply never
                        // copied it across, so no PHP-authored field could be documented.
                        if ($f->description !== null) {
                            $field['description'] = $f->description;
                        }

                        // #807: the exporter emitted only name/type/nullable, so a field
                        // the author gated with #[GraphQLField(scope: '...')] reached the
                        // compiler with no scope at all and was served to callers holding
                        // none. The scope was dropped here, one layer earlier than the
                        // other SDKs' key drift, but with the identical outcome.
                        //
                        // The compiled schema and the runtime field filter represent
                        // exactly one required scope, so a multi-scope declaration cannot
                        // be honoured and is refused rather than silently discarded.
                        if ($f->scopes !== null && count($f->scopes) > 1) {
                            throw new \InvalidArgumentException(sprintf(
                                'Field "%s" requires %d scopes; multiple required scopes are '
                                . 'not supported — declare a single scope.',
                                $f->name,
                                count($f->scopes),
                            ));
                        }

                        $scope = $f->scope ?? ($f->scopes[0] ?? null);
                        if ($scope !== null) {
                            $field['requires_scope'] = $scope;
                        }

                        // A Vector field without its config is refused by the compiler,
                        // so dropping these here would not be a silent loss — it would
                        // make the four pgvector field types unauthorable in PHP.
                        if ($f->vectorConfig !== null) {
                            $field['vector_config'] = $f->vectorConfig->toArray();
                        }

                        if ($f->vectorDistance !== null) {
                            $field['vector_distance'] = $f->vectorDistance;
                        }

                        // `IntermediateField.deprecated` has been readable since #1025.
                        // There was no attribute to put a reason in, so a PHP author
                        // could not deprecate a field at all.
                        if ($f->deprecated !== null) {
                            $field['deprecated'] = ['reason' => $f->deprecated];
                        }

                        return $field;
                    },
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

                // #1266: emitted only when non-empty, so a type declaring none is
                // byte-identical to pre-#1266 output.
                if ($typeAttr->relationships !== []) {
                    $typeDef['relationships'] = array_map(
                        static fn (\FraiseQL\Relationship $r): array => $r->toArray(),
                        $typeAttr->relationships,
                    );
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
