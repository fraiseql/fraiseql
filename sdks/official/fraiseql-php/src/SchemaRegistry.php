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

    /** @var array<string, array{name: string, fields: list<array{name: string, type: string, nullable: bool}>, description: string|null}> Registered input types */
    private array $inputTypes = [];

    /** @var array<string, array{sql_source: string|null, is_error: bool}> Extra type metadata */
    private array $typeMeta = [];

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
            $fields[$property->getName()] = $this->extractFieldDefinition($property, $typeName);
        }

        $this->typeFields[$typeName] = $fields;

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
     * Register a GraphQL input object type.
     *
     * @param string $name Input type name (e.g. "CreateUserInput")
     * @param list<array{name: string, type: string, nullable: bool}> $fields Field definitions
     * @param string|null $description Optional input type description
     * @return self Fluent interface
     *
     * @throws FraiseQLException If an input type with this name is already registered
     */
    public function registerInputType(string $name, array $fields, ?string $description = null): self
    {
        if (isset($this->inputTypes[$name])) {
            throw new FraiseQLException(
                "Input type '{$name}' is already registered. Each name must be unique within a schema.",
            );
        }

        $this->inputTypes[$name] = [
            'name' => $name,
            'fields' => $fields,
            'description' => $description,
        ];

        return $this;
    }

    /**
     * Get all registered input types.
     *
     * @return array<string, array{name: string, fields: list<array{name: string, type: string, nullable: bool}>, description: string|null}>
     */
    public function getAllInputTypes(): array
    {
        return $this->inputTypes;
    }

    /** @var array<string, string> Base inject defaults */
    private array $injectDefaultsBase = [];

    /** @var array<string, string> Query inject defaults */
    private array $injectDefaultsQueries = [];

    /** @var array<string, string> Mutation inject defaults */
    private array $injectDefaultsMutations = [];

    /**
     * Set inject defaults from configuration.
     *
     * @param array<string, string> $base Base defaults for all operations
     * @param array<string, string> $queries Additional defaults for queries
     * @param array<string, string> $mutations Additional defaults for mutations
     * @return void
     */
    public function setInjectDefaults(array $base, array $queries, array $mutations): void
    {
        $this->injectDefaultsBase = $base;
        $this->injectDefaultsQueries = $queries;
        $this->injectDefaultsMutations = $mutations;
    }

    /**
     * Get inject defaults.
     *
     * @return array{base: array<string, string>, queries: array<string, string>, mutations: array<string, string>}
     */
    public function getInjectDefaults(): array
    {
        return [
            'base' => $this->injectDefaultsBase,
            'queries' => $this->injectDefaultsQueries,
            'mutations' => $this->injectDefaultsMutations,
        ];
    }

    /**
     * Get type metadata (sql_source, is_error) for a specific type.
     *
     * @param string $typeName The type name
     * @return array{sql_source: string|null, is_error: bool}|null
     */
    public function getTypeMeta(string $typeName): ?array
    {
        return $this->typeMeta[$typeName] ?? null;
    }

    /**
     * Set type metadata (sql_source, is_error) for a specific type.
     *
     * @param string $typeName The type name
     * @param array{sql_source: string|null, is_error: bool} $meta The metadata
     * @return void
     */
    public function setTypeMeta(string $typeName, array $meta): void
    {
        $this->typeMeta[$typeName] = $meta;
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
        $this->inputTypes = [];
        $this->typeMeta = [];
        $this->injectDefaultsBase = [];
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
