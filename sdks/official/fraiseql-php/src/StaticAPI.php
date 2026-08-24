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
        $registry = SchemaRegistry::getInstance();
        $registry->register($className);

        foreach ($registry->getTypeNames() as $typeName) {
            $attr = $registry->getType($typeName);
            if ($attr !== null && $attr->crud) {
                self::expandCrud($typeName, $registry->getTypeFields($typeName), $attr->sqlSource, $attr->cascade);
            }
        }
    }

    /**
     * Generate the CRUD operations a type declaring `crud` asks for.
     *
     * `CrudGenerator` was fully implemented and had **no callers at all**, so
     * `crud: true` generated nothing through either authoring idiom — the fluent
     * builder or the `#[GraphQLType(crud: true)]` attribute (#1022).
     *
     * The flags are expanded here rather than emitted into the schema on purpose:
     * `IntermediateType` has no `crud`/`cascade` member and denies unknown fields,
     * so they are authoring-time expansion flags, exactly as in the Python SDK.
     * `cascade` rides on the generated **mutations**, which is where
     * `IntermediateMutation::cascade` lives.
     *
     * Expansion happens once, at registration: queries and mutations are keyed by
     * name and would merely overwrite, but `registerInputType` throws on a
     * duplicate name, so expanding again at export time would be a hard failure.
     *
     * @param array<string, FieldDefinition> $fields
     */
    private static function expandCrud(
        string $typeName,
        array $fields,
        ?string $sqlSource,
        bool $cascade,
    ): void {
        if ($fields === []) {
            return;
        }

        $generated = CrudGenerator::generate($typeName, $fields, $sqlSource, $cascade);
        foreach ($generated['queries'] as $query) {
            $query->register();
        }
        foreach ($generated['mutations'] as $mutation) {
            $mutation->register();
        }
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
     * Register a GraphQL enum type.
     *
     * @param string $name Enum type name (e.g. 'OrderStatus')
     * @param list<string> $values Enum member names, in declaration order
     * @param string|null $description Optional enum description
     * @return void
     */
    public static function enum(string $name, array $values, ?string $description = null): void
    {
        SchemaRegistry::getInstance()->registerEnum($name, $values, $description);
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
        /** @var array<string, \FraiseQL\Attributes\GraphQLType> $types */
        $types = $typesProperty->getValue($registry);

        $fieldsProperty = $reflection->getProperty('typeFields');
        $fieldsProperty->setAccessible(true);
        /** @var array<string, array<string, \FraiseQL\FieldDefinition>> $typeFields */
        $typeFields = $fieldsProperty->getValue($registry);

        // Every fact the builder carries goes into the attribute, because the attribute
        // is what `SchemaExporter` reads. `sqlSource` and `isError` used to be diverted
        // into a `SchemaRegistry` side table instead, so a builder-authored type
        // exported through the shipped exporter with no source view and no error flag
        // (#952) — visible only to a second serializer that no shipped path used.
        $typeAttr = new GraphQLType(
            name: $builder->getName(),
            sqlSource: $builder->getSqlSource(),
            description: $builder->getDescription(),
            isError: $builder->getIsError(),
            crud: $builder->getCrud(),
            cascade: $builder->getCascade(),
        );

        $types[$builder->getName()] = $typeAttr;
        $typeFields[$builder->getName()] = $builder->getFields();

        $typesProperty->setValue($registry, $types);
        $fieldsProperty->setValue($registry, $typeFields);

        // The two builder flags had getters and no readers anywhere in the SDK, so a
        // type declared `->crud(true)` generated nothing (#1022). They are expanded
        // through the same helper the attribute path uses — one implementation, not a
        // second copy that can drift.
        if ($builder->getCrud()) {
            self::expandCrud(
                $builder->getName(),
                $builder->getFields(),
                $builder->getSqlSource(),
                $builder->getCascade(),
            );
        }
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
}
