<?php

declare(strict_types=1);

namespace FraiseQL;

use FraiseQL\Attributes\GraphQLType;
use ReflectionClass;

/**
 * Central registry for GraphQL type definitions.
 *
 * The SchemaRegistry manages type registration, lookup, and introspection
 * for the entire GraphQL schema. It uses a singleton pattern suitable for
 * PHP's single-threaded execution model.
 *
 * Usage:
 * ```php
 * $registry = SchemaRegistry::getInstance();
 * $registry->register(User::class);
 * $userType = $registry->getType('User');
 * ```
 */
final class SchemaRegistry
{
    /** @var SchemaRegistry|null */
    private static ?self $instance = null;

    /** @var array<string, GraphQLType> Registered types by name */
    private array $types = [];

    /** @var array<string, string> Class name to GraphQL type name mapping */
    private array $classToTypeName = [];

    /** @var array<string, array<string, FieldDefinition>> Fields for each type */
    private array $typeFields = [];

    /** @var array<string, SubscriptionDefinition> Registered subscriptions */
    private array $subscriptions = [];

    /** @var array<string, QueryBuilder> Registered queries */
    private array $queries = [];

    /** @var array<string, MutationBuilder> Registered mutations */
    private array $mutations = [];

    /** @var array<string, array{sql_source: string|null, is_error: bool}> Extra type metadata */
    private array $typeMeta = [];

    /** @var array<string, bool> Tenant-scoped flags by type name */
    private array $tenantScoped = [];

    /** @var array<string, string> Base inject defaults (param => 'jwt:claim') */
    private array $injectDefaults = [];

    /** @var array<string, string> Query-specific inject defaults */
    private array $injectDefaultsQueries = [];

    /** @var array<string, string> Mutation-specific inject defaults */
    private array $injectDefaultsMutations = [];

    private function __construct()
    {
    }

    /**
     * Get the singleton instance of the SchemaRegistry.
     *
     * @return self
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }

        return self::$instance;
    }

    /**
     * Register a GraphQL type from a PHP class.
     *
     * Extracts type definition from GraphQLType attribute and introspects
     * all properties marked with GraphQLField attributes.
     *
     * @param class-string $className The fully qualified class name
     * @return self Fluent interface
     *
     * @throws FraiseQLException If class doesn't have GraphQLType attribute
     */
    public function register(string $className): self
    {
        $reflection = new ReflectionClass($className);
        $attributes = $reflection->getAttributes(GraphQLType::class);

        if (empty($attributes)) {
            throw new FraiseQLException(
                "Class $className must have #[GraphQLType] attribute",
            );
        }

        /** @var GraphQLType $typeAttribute */
        $typeAttribute = $attributes[0]->newInstance();

        $typeName = $typeAttribute->name ?? $reflection->getShortName();

        // Register type
        $this->types[$typeName] = $typeAttribute;
        $this->classToTypeName[$className] = $typeName;

        // Register fields
        $fields = [];
        foreach ($reflection->getProperties() as $property) {
            $fieldDef = $this->extractFieldDefinition($property, $typeName);
            $fields[$property->getName()] = $fieldDef;
        }

        $this->typeFields[$typeName] = $fields;

        // Store tenant_scoped flag
        if ($typeAttribute->tenantScoped) {
            $this->tenantScoped[$typeName] = true;
        }

        // Generate CRUD operations if requested
        if ($typeAttribute->crud !== false) {
            $this->generateCrudOperations($typeName, $fields, $typeAttribute->crud, $typeAttribute->sqlSource ?? null);
        }

        return $this;
    }

    /**
     * Get a registered type by name.
     *
     * @param string $typeName The GraphQL type name
     * @return GraphQLType|null The type definition, or null if not registered
     */
    public function getType(string $typeName): ?GraphQLType
    {
        return $this->types[$typeName] ?? null;
    }

    /**
     * Get all fields for a type.
     *
     * @param string $typeName The GraphQL type name
     * @return array<string, FieldDefinition> Array of field definitions indexed by field name
     */
    public function getTypeFields(string $typeName): array
    {
        return $this->typeFields[$typeName] ?? [];
    }

    /**
     * Get a specific field definition.
     *
     * @param string $typeName The GraphQL type name
     * @param string $fieldName The field name
     * @return FieldDefinition|null The field definition, or null if not found
     */
    public function getField(string $typeName, string $fieldName): ?FieldDefinition
    {
        return $this->typeFields[$typeName][$fieldName] ?? null;
    }

    /**
     * Check if a type is registered.
     *
     * @param string $typeName The GraphQL type name
     * @return bool
     */
    public function hasType(string $typeName): bool
    {
        return isset($this->types[$typeName]);
    }

    /**
     * Get all registered type names.
     *
     * @return array<string> Array of type names
     */
    public function getTypeNames(): array
    {
        return array_keys($this->types);
    }

    /**
     * Get the GraphQL type name for a PHP class.
     *
     * @param class-string $className The PHP class name
     * @return string|null The GraphQL type name, or null if class not registered
     */
    public function getTypeNameForClass(string $className): ?string
    {
        return $this->classToTypeName[$className] ?? null;
    }

    /**
     * Register a subscription.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     *
     * @param SubscriptionDefinition $subscription The subscription to register
     * @return self Fluent interface
     */
    public function registerSubscription(SubscriptionDefinition $subscription): self
    {
        $this->subscriptions[$subscription->name] = $subscription;
        return $this;
    }

    /**
     * Get a registered subscription by name.
     *
     * @param string $name The subscription name
     * @return SubscriptionDefinition|null The subscription or null if not found
     */
    public function getSubscription(string $name): ?SubscriptionDefinition
    {
        return $this->subscriptions[$name] ?? null;
    }

    /**
     * Get all registered subscriptions.
     *
     * @return array<string, SubscriptionDefinition>
     */
    public function getAllSubscriptions(): array
    {
        return $this->subscriptions;
    }

    /**
     * Check if a subscription is registered.
     *
     * @param string $name The subscription name
     * @return bool
     */
    public function hasSubscription(string $name): bool
    {
        return isset($this->subscriptions[$name]);
    }


    /**
     * Register a query definition.
     *
     * @param QueryBuilder $query The query builder
     * @return self Fluent interface
     */
    public function registerQuery(QueryBuilder $query): self
    {
        $this->queries[$query->getName()] = $query;
        return $this;
    }

    /**
     * Get a registered query by name.
     *
     * @param string $name The query name
     * @return QueryBuilder|null
     */
    public function getQuery(string $name): ?QueryBuilder
    {
        return $this->queries[$name] ?? null;
    }

    /**
     * Get all registered queries.
     *
     * @return array<string, QueryBuilder>
     */
    public function getAllQueries(): array
    {
        return $this->queries;
    }

    /**
     * Register a mutation definition.
     *
     * @param MutationBuilder $mutation The mutation builder
     * @return self Fluent interface
     */
    public function registerMutation(MutationBuilder $mutation): self
    {
        $this->mutations[$mutation->getName()] = $mutation;
        return $this;
    }

    /**
     * Get a registered mutation by name.
     *
     * @param string $name The mutation name
     * @return MutationBuilder|null
     */
    public function getMutation(string $name): ?MutationBuilder
    {
        return $this->mutations[$name] ?? null;
    }

    /**
     * Get all registered mutations.
     *
     * @return array<string, MutationBuilder>
     */
    public function getAllMutations(): array
    {
        return $this->mutations;
    }

    /**
     * Get type metadata (sql_source, is_error) for a given type.
     *
     * @param string $typeName The GraphQL type name
     * @return array{sql_source: string|null, is_error: bool}|null
     */
    public function getTypeMeta(string $typeName): ?array
    {
        return $this->typeMeta[$typeName] ?? null;
    }

    /**
     * Set type metadata for a given type.
     *
     * @param string $typeName The GraphQL type name
     * @param array{sql_source: string|null, is_error: bool} $meta
     */
    public function setTypeMeta(string $typeName, array $meta): void
    {
        $this->typeMeta[$typeName] = $meta;
    }

    /**
     * Check if a type is tenant-scoped.
     *
     * @param string $typeName The GraphQL type name
     * @return bool
     */
    public function isTenantScoped(string $typeName): bool
    {
        return $this->tenantScoped[$typeName] ?? false;
    }

    /**
     * Set inject defaults for queries and mutations.
     *
     * @param array<string, string> $base Base inject defaults applied to all operations
     * @param array<string, string> $queries Additional inject defaults for queries only
     * @param array<string, string> $mutations Additional inject defaults for mutations only
     * @return self Fluent interface
     */
    public function setInjectDefaults(array $base, array $queries = [], array $mutations = []): self
    {
        $this->injectDefaults = $base;
        $this->injectDefaultsQueries = $queries;
        $this->injectDefaultsMutations = $mutations;
        return $this;
    }

    /**
     * Get base inject defaults.
     *
     * @return array<string, string>
     */
    public function getInjectDefaults(): array
    {
        return $this->injectDefaults;
    }

    /**
     * Get query-specific inject defaults.
     *
     * @return array<string, string>
     */
    public function getInjectDefaultsQueries(): array
    {
        return $this->injectDefaultsQueries;
    }

    /**
     * Get mutation-specific inject defaults.
     *
     * @return array<string, string>
     */
    public function getInjectDefaultsMutations(): array
    {
        return $this->injectDefaultsMutations;
    }

    /**
     * Generate CRUD operations for a type.
     *
     * @param string $typeName The GraphQL type name
     * @param array<string, FieldDefinition> $fields The type's field definitions
     * @param array<string>|bool $crud CRUD configuration: true, ['all'], or list of operations
     */
    private function generateCrudOperations(string $typeName, array $fields, array|bool $crud, ?string $sqlSource = null): void
    {
        $snake = self::pascalToSnake($typeName);
        $view = $sqlSource ?? ('v_' . $snake);

        // Determine which operations to generate
        $ops = [];
        if ($crud === true) {
            $ops = ['read', 'create', 'update', 'delete'];
        } elseif (is_array($crud)) {
            if (in_array('all', $crud, true)) {
                $ops = ['read', 'create', 'update', 'delete'];
            } else {
                $ops = $crud;
            }
        }

        // Find PK field (first field starting with pk_)
        $pkField = null;
        foreach ($fields as $field) {
            if (str_starts_with($field->name, 'pk_')) {
                $pkField = $field;
                break;
            }
        }

        if (in_array('read', $ops, true)) {
            $this->generateReadOperations($typeName, $snake, $view, $fields, $pkField);
        }

        if (in_array('create', $ops, true)) {
            $this->generateCreateOperation($typeName, $snake, $fields);
        }

        if (in_array('update', $ops, true) && $pkField !== null) {
            $this->generateUpdateOperation($typeName, $snake, $fields, $pkField);
        }

        if (in_array('delete', $ops, true) && $pkField !== null) {
            $this->generateDeleteOperation($typeName, $snake, $pkField);
        }
    }

    /**
     * Generate read operations (get by ID + list).
     *
     * @param string $typeName The GraphQL type name
     * @param string $snake The snake_case name
     * @param string $view The view name
     * @param array<string, FieldDefinition> $fields The type's fields
     * @param FieldDefinition|null $pkField The primary key field
     */
    private function generateReadOperations(
        string $typeName,
        string $snake,
        string $view,
        array $fields,
        ?FieldDefinition $pkField,
    ): void {
        // Get by ID query (only if PK exists)
        if ($pkField !== null) {
            $getQuery = QueryBuilder::query($snake)
                ->returnType($typeName)
                ->returnsList(false)
                ->sqlSource($view)
                ->argument($pkField->name, $pkField->type, nullable: false);
            $this->registerQuery($getQuery);
        }

        // List query with auto_params
        $listQuery = QueryBuilder::query(self::pluralize($snake))
            ->returnType($typeName)
            ->returnsList(true)
            ->sqlSource($view)
            ->autoParams(true);
        $this->registerQuery($listQuery);
    }

    /**
     * Generate create mutation.
     *
     * @param string $typeName The GraphQL type name
     * @param string $snake The snake_case name
     * @param array<string, FieldDefinition> $fields The type's fields
     */
    private function generateCreateOperation(string $typeName, string $snake, array $fields): void
    {
        $mutation = MutationBuilder::mutation('create' . $typeName)
            ->returnType($typeName)
            ->sqlSource('fn_create_' . $snake)
            ->operation('insert');

        foreach ($fields as $field) {
            $mutation->argument($field->name, $field->type, nullable: $field->nullable);
        }

        $this->registerMutation($mutation);
    }

    /**
     * Generate update mutation.
     *
     * @param string $typeName The GraphQL type name
     * @param string $snake The snake_case name
     * @param array<string, FieldDefinition> $fields The type's fields
     * @param FieldDefinition $pkField The primary key field
     */
    private function generateUpdateOperation(
        string $typeName,
        string $snake,
        array $fields,
        FieldDefinition $pkField,
    ): void {
        $mutation = MutationBuilder::mutation('update' . $typeName)
            ->returnType($typeName)
            ->sqlSource('fn_update_' . $snake)
            ->operation('update');

        // PK is required
        $mutation->argument($pkField->name, $pkField->type, nullable: false);

        // Other fields are nullable (optional for update)
        foreach ($fields as $field) {
            if ($field->name === $pkField->name) {
                continue;
            }
            $mutation->argument($field->name, $field->type, nullable: true);
        }

        $this->registerMutation($mutation);
    }

    /**
     * Generate delete mutation.
     *
     * @param string $typeName The GraphQL type name
     * @param string $snake The snake_case name
     * @param FieldDefinition $pkField The primary key field
     */
    private function generateDeleteOperation(string $typeName, string $snake, FieldDefinition $pkField): void
    {
        $mutation = MutationBuilder::mutation('delete' . $typeName)
            ->returnType($typeName)
            ->sqlSource('fn_delete_' . $snake)
            ->operation('delete');

        $mutation->argument($pkField->name, $pkField->type, nullable: false);

        $this->registerMutation($mutation);
    }

    /**
     * Pluralize a snake_case name using basic English rules.
     *
     * Rules (ordered):
     * 1. Already ends in 's' (but not 'ss') -> no change (e.g. 'statistics')
     * 2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
     * 3. Ends in consonant + 'y' -> replace 'y' with 'ies'
     * 4. Default -> append 's'
     *
     * @param string $name The name to pluralize
     * @return string The pluralized name
     */
    private static function pluralize(string $name): string
    {
        if (str_ends_with($name, 's') && !str_ends_with($name, 'ss')) {
            return $name;
        }
        if (preg_match('/(?:ss|sh|ch|x|z)$/', $name)) {
            return $name . 'es';
        }
        if (strlen($name) >= 2 && str_ends_with($name, 'y') && !str_contains('aeiou', $name[strlen($name) - 2])) {
            return substr($name, 0, -1) . 'ies';
        }
        return $name . 's';
    }

    /**
     * Convert PascalCase to snake_case.
     *
     * @param string $name The PascalCase name
     * @return string The snake_case name
     */
    private static function pascalToSnake(string $name): string
    {
        $result = preg_replace('/(?<!^)[A-Z]/', '_$0', $name);

        return strtolower($result ?? $name);
    }

    /**
     * Clear all registered types (useful for testing).
     *
     * @return self Fluent interface
     */
    public function clear(): self
    {
        $this->types = [];
        $this->classToTypeName = [];
        $this->typeFields = [];
        $this->subscriptions = [];
        $this->queries = [];
        $this->mutations = [];
        $this->typeMeta = [];
        $this->tenantScoped = [];
        $this->injectDefaults = [];
        $this->injectDefaultsQueries = [];
        $this->injectDefaultsMutations = [];

        return $this;
    }

    /**
     * Extract field definition from a ReflectionProperty.
     *
     * @param \ReflectionProperty $property The property to extract from
     * @param string $typeName The parent type name
     * @return FieldDefinition The field definition
     */
    private function extractFieldDefinition(
        \ReflectionProperty $property,
        string $typeName,
    ): FieldDefinition {
        $typeInfo = TypeConverter::fromReflectionProperty($property);

        return new FieldDefinition(
            name: $property->getName(),
            type: $typeInfo->graphQLType,
            nullable: $typeInfo->isNullable,
            isList: $typeInfo->isList,
            description: $typeInfo->description,
            phpType: $typeInfo->phpType,
            customResolver: $typeInfo->customResolver,
            parentType: $typeName,
        );
    }
}
