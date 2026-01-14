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
            if ($fieldDef !== null) {
                $fields[$property->getName()] = $fieldDef;
            }
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
     * Clear all registered types (useful for testing).
     *
     * @return self Fluent interface
     */
    public function clear(): self
    {
        $this->types = [];
        $this->classToTypeName = [];
        $this->typeFields = [];

        return $this;
    }

    /**
     * Extract field definition from a ReflectionProperty.
     *
     * @param \ReflectionProperty $property The property to extract from
     * @param string $typeName The parent type name
     * @return FieldDefinition|null The field definition, or null if not a GraphQL field
     */
    private function extractFieldDefinition(
        \ReflectionProperty $property,
        string $typeName,
    ): ?FieldDefinition {
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
