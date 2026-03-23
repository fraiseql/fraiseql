<?php

declare(strict_types=1);

namespace FraiseQL;

use FraiseQL\Attributes\GraphQLType;

/**
 * Static API for easy schema construction and configuration.
 *
 * Provides a convenient static interface to the SchemaRegistry and builder
 * pattern for creating GraphQL schemas with a fluent API.
 *
 * Usage:
 * ```php
 * // Register types from classes with attributes
 * StaticAPI::register(User::class);
 * StaticAPI::register(Product::class);
 *
 * // Register types using builder
 * StaticAPI::type('Query')
 *     ->field('user', 'User')
 *     ->field('users', 'User', isList: true)
 *     ->build();
 *
 * // Get type information
 * $type = StaticAPI::getType('User');
 * $fields = StaticAPI::getTypeFields('User');
 * ```
 */
final class StaticAPI
{
    /**
     * Register a type from a PHP class with #[GraphQLType] attribute.
     *
     * @param class-string $className The fully qualified class name
     * @return void
     *
     * @throws FraiseQLException If class doesn't have GraphQLType attribute
     */
    public static function register(string $className): void
    {
        SchemaRegistry::getInstance()->register($className);
    }

    /**
     * Start building a type definition.
     *
     * @param string $name The GraphQL type name
     * @return TypeBuilder The type builder
     */
    public static function type(string $name): TypeBuilder
    {
        return TypeBuilder::type($name);
    }

    /**
     * Start building a subscription definition.
     * Subscriptions in FraiseQL are compiled projections of database events.
     * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
     *
     * @param string $name The subscription name
     * @return SubscriptionBuilder The subscription builder
     */
    public static function subscription(string $name): SubscriptionBuilder
    {
        return SubscriptionBuilder::subscription($name);
    }

    /**
     * Register a type from a TypeBuilder instance.
     *
     * @param TypeBuilder $builder The type builder
     * @return void
     */
    public static function registerBuilder(TypeBuilder $builder): void
    {
        $registry = SchemaRegistry::getInstance();

        // Create a temporary GraphQL type attribute
        // We'll store it directly in the registry
        $reflection = new \ReflectionClass($registry);
        $typesProperty = $reflection->getProperty('types');
        $typesProperty->setAccessible(true);
        $types = $typesProperty->getValue($registry);

        $fieldsProperty = $reflection->getProperty('typeFields');
        $fieldsProperty->setAccessible(true);
        $typeFields = $fieldsProperty->getValue($registry);

        // Store the type with a proper GraphQLType instance
        $types[$builder->getName()] = new GraphQLType(
            name: $builder->getName(),
            description: $builder->getDescription(),
        );
        $typeFields[$builder->getName()] = $builder->getFields();

        $typesProperty->setValue($registry, $types);
        $fieldsProperty->setValue($registry, $typeFields);
    }

    /**
     * Get a registered type by name.
     *
     * @param string $typeName The GraphQL type name
     * @return mixed The type definition or null
     */
    public static function getType(string $typeName): mixed
    {
        return SchemaRegistry::getInstance()->getType($typeName);
    }

    /**
     * Get all fields for a type.
     *
     * @param string $typeName The GraphQL type name
     * @return array<string, FieldDefinition>
     */
    public static function getTypeFields(string $typeName): array
    {
        return SchemaRegistry::getInstance()->getTypeFields($typeName);
    }

    /**
     * Get a specific field definition.
     *
     * @param string $typeName The GraphQL type name
     * @param string $fieldName The field name
     * @return FieldDefinition|null
     */
    public static function getField(string $typeName, string $fieldName): ?FieldDefinition
    {
        return SchemaRegistry::getInstance()->getField($typeName, $fieldName);
    }

    /**
     * Check if a type is registered.
     *
     * @param string $typeName The GraphQL type name
     * @return bool
     */
    public static function hasType(string $typeName): bool
    {
        return SchemaRegistry::getInstance()->hasType($typeName);
    }

    /**
     * Get all registered type names.
     *
     * @return array<string>
     */
    public static function getTypeNames(): array
    {
        return SchemaRegistry::getInstance()->getTypeNames();
    }

    /**
     * Get the GraphQL type name for a PHP class.
     *
     * @param class-string $className The PHP class name
     * @return string|null
     */
    public static function getTypeNameForClass(string $className): ?string
    {
        return SchemaRegistry::getInstance()->getTypeNameForClass($className);
    }

    /**
     * Get a registered subscription by name.
     *
     * @param string $name The subscription name
     * @return SubscriptionDefinition|null
     */
    public static function getSubscription(string $name): ?SubscriptionDefinition
    {
        return SchemaRegistry::getInstance()->getSubscription($name);
    }

    /**
     * Get all registered subscriptions.
     *
     * @return array<string, SubscriptionDefinition>
     */
    public static function getAllSubscriptions(): array
    {
        return SchemaRegistry::getInstance()->getAllSubscriptions();
    }

    /**
     * Check if a subscription is registered.
     *
     * @param string $name The subscription name
     * @return bool
     */
    public static function hasSubscription(string $name): bool
    {
        return SchemaRegistry::getInstance()->hasSubscription($name);
    }

    /**
     * Start building a query definition.
     *
     * @param string $name The query name
     * @return QueryBuilder The query builder
     */
    public static function query(string $name): QueryBuilder
    {
        return QueryBuilder::query($name);
    }

    /**
     * Start building a mutation definition.
     *
     * @param string $name The mutation name
     * @return MutationBuilder The mutation builder
     */
    public static function mutation(string $name): MutationBuilder
    {
        return MutationBuilder::mutation($name);
    }

    /**
     * Register a TypeBuilder instance (including sql_source and is_error metadata).
     *
     * @param TypeBuilder $builder The type builder
     * @return void
     */
    public static function registerTypeBuilder(TypeBuilder $builder): void
    {
        $registry = SchemaRegistry::getInstance();

        $reflection = new \ReflectionClass($registry);

        $typesProperty = $reflection->getProperty('types');
        $typesProperty->setAccessible(true);
        $types = $typesProperty->getValue($registry);

        $fieldsProperty = $reflection->getProperty('typeFields');
        $fieldsProperty->setAccessible(true);
        $typeFields = $fieldsProperty->getValue($registry);

        $typeAttr = new \FraiseQL\Attributes\GraphQLType(
            name: $builder->getName(),
            description: $builder->getDescription(),
        );

        $types[$builder->getName()] = $typeAttr;
        $typeFields[$builder->getName()] = $builder->getFields();

        $typesProperty->setValue($registry, $types);
        $fieldsProperty->setValue($registry, $typeFields);

        // Store sql_source and is_error metadata
        $registry->setTypeMeta($builder->getName(), [
            'sql_source' => $builder->getSqlSource(),
            'is_error'   => $builder->getIsError(),
        ]);
    }

    /**
     * Export the complete schema as an array (types + queries + mutations).
     *
     * @return array<string, mixed>
     */
    public static function exportSchema(): array
    {
        $registry = SchemaRegistry::getInstance();

        // Build types section
        $types = [];
        foreach ($registry->getTypeNames() as $typeName) {
            $typeAttr = $registry->getType($typeName);
            $fields   = $registry->getTypeFields($typeName);

            $typeDef = [
                'name'   => $typeName,
                'fields' => array_map(
                    static fn (\FraiseQL\FieldDefinition $f) => [
                        'name'     => $f->name,
                        'type'     => $f->type,
                        'nullable' => $f->nullable,
                    ],
                    $fields,
                ),
            ];

            if ($typeAttr !== null && $typeAttr->description !== null) {
                $typeDef['description'] = $typeAttr->description;
            }

            // Retrieve typeMeta if available
            $meta = self::getTypeMeta($registry, $typeName);
            if ($meta !== null) {
                if ($meta['sql_source'] !== null) {
                    $typeDef['sql_source'] = $meta['sql_source'];
                }
                if ($meta['is_error']) {
                    $typeDef['is_error'] = true;
                }
            }

            $types[$typeName] = $typeDef;
        }

        // Build queries section
        $queries = [];
        foreach ($registry->getAllQueries() as $name => $builder) {
            $queries[$name] = $builder->toArray();
        }

        // Build mutations section
        $mutations = [];
        foreach ($registry->getAllMutations() as $name => $builder) {
            $mutations[$name] = $builder->toArray();
        }

        $schema = ['version' => '1.0'];
        if (!empty($types)) {
            $schema['types'] = $types;
        }
        if (!empty($queries)) {
            $schema['queries'] = $queries;
        }
        if (!empty($mutations)) {
            $schema['mutations'] = $mutations;
        }

        return $schema;
    }

    /**
     * Clear all registered types (useful for testing).
     *
     * @return void
     */
    public static function clear(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    /**
     * @return array{sql_source: string|null, is_error: bool}|null
     */
    private static function getTypeMeta(SchemaRegistry $registry, string $typeName): ?array
    {
        return $registry->getTypeMeta($typeName);
    }
}
