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
     * @throws \Exception If scope validation fails
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

        // Validate scopes if present
        if ($graphQLFieldAttribute !== null) {
            if ($graphQLFieldAttribute->scope !== null && $graphQLFieldAttribute->scopes !== null) {
                throw new \Exception(
                    "Field {$property->getName()} cannot have both scope and scopes"
                );
            }

            if ($graphQLFieldAttribute->scope !== null) {
                self::validateScope($graphQLFieldAttribute->scope, $property->getName());
            }

            if ($graphQLFieldAttribute->scopes !== null) {
                if (empty($graphQLFieldAttribute->scopes)) {
                    throw new \Exception(
                        "Field {$property->getName()} has empty scopes array"
                    );
                }
                foreach ($graphQLFieldAttribute->scopes as $scope) {
                    if (empty($scope)) {
                        throw new \Exception(
                            "Field {$property->getName()} has empty scope in scopes array"
                        );
                    }
                    self::validateScope($scope, $property->getName());
                }
            }
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
                scope: $graphQLFieldAttribute->scope,
                scopes: $graphQLFieldAttribute->scopes,
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
            scope: $graphQLFieldAttribute?->scope,
            scopes: $graphQLFieldAttribute?->scopes,
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
                    scope: $fieldAttribute?->scope,
                    scopes: $fieldAttribute?->scopes,
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
                    scope: $fieldAttribute?->scope,
                    scopes: $fieldAttribute?->scopes,
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
                scope: $fieldAttribute?->scope,
                scopes: $fieldAttribute?->scopes,
            );
        }

        // Fallback
        return new TypeInfo(
            phpType: 'mixed',
            graphQLType: 'String',
            isNullable: true,
            description: $fieldAttribute?->description,
            customResolver: $fieldAttribute?->resolver,
            scope: $fieldAttribute?->scope,
            scopes: $fieldAttribute?->scopes,
        );
    }

    /**
     * Validate scope format: action:resource
     * Valid patterns:
     * - * (global wildcard)
     * - action:resource (read:user.email, write:User.salary)
     * - action:* (admin:*, read:*)
     *
     * @param string $scope The scope to validate
     * @param string $fieldName The field name for error reporting
     * @throws \Exception If scope format is invalid
     */
    private static function validateScope(string $scope, string $fieldName): void
    {
        if (empty($scope)) {
            throw new \Exception("Field {$fieldName} has empty scope");
        }

        // Global wildcard is always valid
        if ($scope === '*') {
            return;
        }

        // Must contain at least one colon
        if (!str_contains($scope, ':')) {
            throw new \Exception(
                "Field {$fieldName} has invalid scope '{$scope}' (missing colon)"
            );
        }

        [$action, $resource] = explode(':', $scope, 2);

        // Validate action: [a-zA-Z_][a-zA-Z0-9_]*
        if (!self::isValidAction($action)) {
            throw new \Exception(
                "Field {$fieldName} has invalid action in scope '{$scope}' (must be alphanumeric + underscore)"
            );
        }

        // Validate resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
        if (!self::isValidResource($resource)) {
            throw new \Exception(
                "Field {$fieldName} has invalid resource in scope '{$scope}' (must be alphanumeric + underscore + dot, or *)"
            );
        }
    }

    /**
     * Check if action matches [a-zA-Z_][a-zA-Z0-9_]*
     */
    private static function isValidAction(string $action): bool
    {
        if (empty($action)) {
            return false;
        }

        // First character must be letter or underscore
        $firstChar = $action[0];
        if (!ctype_alpha($firstChar) && $firstChar !== '_') {
            return false;
        }

        // Rest must be letters, digits, or underscores
        for ($i = 1; $i < strlen($action); $i++) {
            $char = $action[$i];
            if (!ctype_alnum($char) && $char !== '_') {
                return false;
            }
        }

        return true;
    }

    /**
     * Check if resource matches [a-zA-Z_][a-zA-Z0-9_.]*|*
     */
    private static function isValidResource(string $resource): bool
    {
        if ($resource === '*') {
            return true;
        }

        if (empty($resource)) {
            return false;
        }

        // First character must be letter or underscore
        $firstChar = $resource[0];
        if (!ctype_alpha($firstChar) && $firstChar !== '_') {
            return false;
        }

        // Rest must be letters, digits, underscores, or dots
        for ($i = 1; $i < strlen($resource); $i++) {
            $char = $resource[$i];
            if (!ctype_alnum($char) && $char !== '_' && $char !== '.') {
                return false;
            }
        }

        return true;
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
