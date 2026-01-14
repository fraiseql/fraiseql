<?php

declare(strict_types=1);

namespace FraiseQL;

use ReflectionProperty;
use ReflectionType;
use ReflectionUnionType;
use ReflectionNamedType;
use FraiseQL\Attributes\GraphQLField;

/**
 * Converts PHP types to GraphQL types with reflection and attribute support.
 *
 * This class handles:
 * - Built-in type conversion (int → Int, string → String, etc.)
 * - Custom class type detection
 * - Nullable type handling
 * - List/array type detection
 * - GraphQL attribute parsing
 */
final class TypeConverter
{
    /**
     * Mapping from PHP type names to GraphQL scalar types.
     */
    private const PHP_TO_GRAPHQL_MAP = [
        'int' => 'Int',
        'string' => 'String',
        'bool' => 'Boolean',
        'float' => 'Float',
        'double' => 'Float',
        'mixed' => 'String',
    ];

    /**
     * Convert a ReflectionProperty to TypeInfo by analyzing its type hints and attributes.
     *
     * @param ReflectionProperty $property The property to convert
     * @return TypeInfo The converted type information
     */
    public static function fromReflectionProperty(ReflectionProperty $property): TypeInfo
    {
        // Check for GraphQLField attribute
        $graphQLFieldAttribute = null;
        foreach ($property->getAttributes(GraphQLField::class) as $attribute) {
            /** @var GraphQLField $graphQLFieldAttribute */
            $graphQLFieldAttribute = $attribute->newInstance();
            break;
        }

        $type = $property->getType();

        // If explicit type is provided in attribute, use it
        if ($graphQLFieldAttribute !== null && $graphQLFieldAttribute->type !== null) {
            return new TypeInfo(
                phpType: $property->getName(),
                graphQLType: $graphQLFieldAttribute->type,
                isNullable: $graphQLFieldAttribute->nullable,
                description: $graphQLFieldAttribute->description,
                customResolver: $graphQLFieldAttribute->resolver,
            );
        }

        // Otherwise, extract from reflection
        if ($type !== null) {
            return self::fromReflectionType($type, $graphQLFieldAttribute);
        }

        // Fallback for untyped properties
        return new TypeInfo(
            phpType: 'mixed',
            graphQLType: 'String',
            isNullable: true,
            description: $graphQLFieldAttribute?->description,
            customResolver: $graphQLFieldAttribute?->resolver,
        );
    }

    /**
     * Convert a ReflectionType to TypeInfo.
     *
     * @param ReflectionType $type The reflection type to convert
     * @param GraphQLField|null $fieldAttribute Optional field attribute for additional metadata
     * @return TypeInfo
     */
    public static function fromReflectionType(
        ReflectionType $type,
        ?GraphQLField $fieldAttribute = null,
    ): TypeInfo {
        $isNullable = $type->allowsNull();
        $isList = false;

        if ($type instanceof ReflectionUnionType) {
            // Handle union types
            $types = $type->getTypes();
            $nonNullTypes = array_filter($types, static fn ($t) => $t->getName() !== 'null');

            if (count($nonNullTypes) === 1) {
                // Union with null = nullable
                $type = current($nonNullTypes);
                $isNullable = true;
            } else {
                // Multiple non-null types = use String as fallback
                return new TypeInfo(
                    phpType: 'mixed',
                    graphQLType: 'String',
                    isNullable: true,
                    description: $fieldAttribute?->description,
                    customResolver: $fieldAttribute?->resolver,
                );
            }
        }

        if ($type instanceof ReflectionNamedType) {
            $typeName = $type->getName();

            // Handle array type
            if ($typeName === 'array') {
                return new TypeInfo(
                    phpType: 'array',
                    graphQLType: $fieldAttribute?->type ?? 'String',
                    isNullable: $isNullable,
                    isList: true,
                    description: $fieldAttribute?->description,
                    customResolver: $fieldAttribute?->resolver,
                );
            }

            // Handle built-in types
            $graphQLType = self::PHP_TO_GRAPHQL_MAP[$typeName] ?? $typeName;

            return new TypeInfo(
                phpType: $typeName,
                graphQLType: $graphQLType,
                isNullable: $isNullable,
                description: $fieldAttribute?->description,
                customResolver: $fieldAttribute?->resolver,
            );
        }

        // Fallback
        return new TypeInfo(
            phpType: 'mixed',
            graphQLType: 'String',
            isNullable: true,
            description: $fieldAttribute?->description,
            customResolver: $fieldAttribute?->resolver,
        );
    }

    /**
     * Convert a PHP type string (including unions) to TypeInfo.
     *
     * Examples:
     * - "int" → TypeInfo(phpType: 'int', graphQLType: 'Int', isNullable: false)
     * - "?string" → TypeInfo(phpType: 'string', graphQLType: 'String', isNullable: true)
     * - "User|null" → TypeInfo(phpType: 'User', graphQLType: 'User', isNullable: true)
     *
     * @param string $typeString The PHP type string to convert
     * @return TypeInfo
     */
    public static function fromTypeString(string $typeString): TypeInfo
    {
        return TypeInfo::fromString($typeString);
    }

    /**
     * Check if a PHP type is a built-in scalar type.
     *
     * @param string $typeName The type name to check
     * @return bool
     */
    public static function isScalarType(string $typeName): bool
    {
        return isset(self::PHP_TO_GRAPHQL_MAP[$typeName]);
    }

    /**
     * Check if a type is a list/array type.
     *
     * @param ReflectionType|null $type The type to check
     * @return bool
     */
    public static function isListType(?ReflectionType $type): bool
    {
        if ($type instanceof ReflectionNamedType) {
            return $type->getName() === 'array';
        }

        return false;
    }
}
