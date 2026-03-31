<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * CRUD operation generation for FraiseQL types.
 *
 * When `crud` is enabled on a type, this class auto-generates standard
 * GraphQL queries (get-by-ID, list) and mutations (create, update, delete)
 * following FraiseQL naming conventions.
 */
final class CrudGenerator
{
    /**
     * Convert a PascalCase name to snake_case.
     *
     * @param string $name PascalCase name (e.g. 'OrderItem')
     * @return string snake_case name (e.g. 'order_item')
     */
    public static function pascalToSnake(string $name): string
    {
        return strtolower((string) preg_replace('/(?<!^)([A-Z])/', '_$1', $name));
    }

    /**
     * Apply basic English pluralization rules to a snake_case name.
     *
     * Rules (ordered):
     *  1. Already ends in 's' (but not 'ss') -> no change (e.g. 'statistics')
     *  2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
     *  3. Ends in consonant + 'y' -> replace 'y' with 'ies'
     *  4. Default -> append 's'
     *
     * @param string $name The name to pluralize
     * @return string The pluralized name
     */
    public static function pluralize(string $name): string
    {
        if (str_ends_with($name, 's') && !str_ends_with($name, 'ss')) {
            return $name;
        }
        foreach (['ss', 'sh', 'ch', 'x', 'z'] as $suffix) {
            if (str_ends_with($name, $suffix)) {
                return $name . 'es';
            }
        }
        if (strlen($name) >= 2 && $name[-1] === 'y' && !str_contains('aeiou', $name[-2])) {
            return substr($name, 0, -1) . 'ies';
        }
        return $name . 's';
    }

    /**
     * Generate CRUD queries and mutations for a type.
     *
     * @param string $typeName The GraphQL type name (PascalCase)
     * @param array<string, FieldDefinition> $fields The type's field definitions
     * @param string|null $sqlSource Override SQL view name (defaults to 'v_{snake}')
     * @param bool $cascade Whether generated mutations include cascade support
     * @return array{queries: list<QueryBuilder>, mutations: list<MutationBuilder>}
     *
     * @throws FraiseQLException If the type has no fields
     */
    public static function generate(
        string $typeName,
        array $fields,
        ?string $sqlSource = null,
        bool $cascade = false,
    ): array {
        if (empty($fields)) {
            throw new FraiseQLException("Type '{$typeName}' has no fields; cannot generate CRUD operations");
        }

        $snake = self::pascalToSnake($typeName);
        $view = $sqlSource ?? "v_{$snake}";
        $fieldList = array_values($fields);
        $pkField = $fieldList[0];

        $queries = [];
        $mutations = [];

        // Get-by-ID query
        $queries[] = QueryBuilder::query($snake)
            ->returnType($typeName)
            ->nullable(true)
            ->argument($pkField->name, $pkField->type, nullable: false)
            ->description("Get {$typeName} by ID.")
            ->sqlSource($view);

        // List query with auto_params
        $queries[] = QueryBuilder::query(self::pluralize($snake))
            ->returnType($typeName)
            ->returnsList(true)
            ->description("List {$typeName} records.")
            ->sqlSource($view)
            ->autoParams(true);

        // Create mutation
        $create = MutationBuilder::mutation("create_{$snake}")
            ->returnType($typeName)
            ->description("Create a new {$typeName}.")
            ->sqlSource("fn_create_{$snake}")
            ->operation('INSERT');
        foreach ($fieldList as $f) {
            $create->argument($f->name, $f->type, nullable: $f->nullable);
        }
        if ($cascade) {
            $create->cascade(true);
        }
        $mutations[] = $create;

        // Update mutation (PK required, other fields nullable)
        $update = MutationBuilder::mutation("update_{$snake}")
            ->returnType($typeName)
            ->description("Update an existing {$typeName}.")
            ->sqlSource("fn_update_{$snake}")
            ->operation('UPDATE');
        $update->argument($pkField->name, $pkField->type, nullable: false);
        foreach (array_slice($fieldList, 1) as $f) {
            $update->argument($f->name, $f->type, nullable: true);
        }
        if ($cascade) {
            $update->cascade(true);
        }
        $mutations[] = $update;

        // Delete mutation (PK only)
        $delete = MutationBuilder::mutation("delete_{$snake}")
            ->returnType($typeName)
            ->description("Delete a {$typeName}.")
            ->sqlSource("fn_delete_{$snake}")
            ->operation('DELETE')
            ->argument($pkField->name, $pkField->type, nullable: false);
        if ($cascade) {
            $delete->cascade(true);
        }
        $mutations[] = $delete;

        return ['queries' => $queries, 'mutations' => $mutations];
    }
}
