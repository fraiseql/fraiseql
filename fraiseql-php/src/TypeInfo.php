<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Represents metadata about a PHP type including GraphQL mapping information.
 *
 * This class holds information about type conversions, nullability, and descriptions
 * extracted from PHP 8 attributes and reflection.
 */
final readonly class TypeInfo
{
    /**
     * @param string $phpType The native PHP type (int, string, bool, float, array, mixed, or class name)
     * @param string $graphQLType The corresponding GraphQL type (Int, String, Boolean, Float, etc.)
     * @param bool $isNullable Whether the type is nullable in GraphQL
     * @param bool $isList Whether the type represents a list/array
     * @param string|null $description Optional description for the type
     * @param string|null $customResolver Optional custom resolver method name
     * @param string|null $scope Optional JWT scope required to access this field
     * @param array<string>|null $scopes Optional JWT scopes required to access this field
     */
    public function __construct(
        public string $phpType,
        public string $graphQLType,
        public bool $isNullable = false,
        public bool $isList = false,
        public ?string $description = null,
        public ?string $customResolver = null,
        public ?string $scope = null,
        public ?array $scopes = null,
    ) {
    }

    /**
     * Create a TypeInfo instance from a PHP type string.
     *
     * Handles:
     * - Built-in types: int, string, bool, float, array
     * - Nullable types: ?int, int|null
     * - List types: array<int>, int[]
     * - Class names: User, Product, etc.
     *
     * @param string $typeString PHP type string or class name
     * @param bool $isNullable Whether the type should be nullable
     * @return self
     */
    public static function fromString(string $typeString, bool $isNullable = false): self
    {
        $phpType = $typeString;
        $isList = false;

        // Handle nullable syntax
        if (str_starts_with($typeString, '?')) {
            $phpType = substr($typeString, 1);
            $isNullable = true;
        }

        // Handle union types with null
        if (str_contains($typeString, '|null')) {
            $phpType = str_replace('|null', '', $typeString);
            $isNullable = true;
        }

        // Handle array notation
        if (str_ends_with($phpType, '[]')) {
            $phpType = substr($phpType, 0, -2);
            $isList = true;
        }

        // Handle array<Type> notation
        if (str_starts_with($phpType, 'array<') && str_ends_with($phpType, '>')) {
            $phpType = substr($phpType, 6, -1);
            $isList = true;
        }

        $graphQLType = self::phpTypeToGraphQL($phpType);

        return new self(
            phpType: $phpType,
            graphQLType: $graphQLType,
            isNullable: $isNullable,
            isList: $isList,
        );
    }

    /**
     * Convert a PHP type to its GraphQL equivalent.
     *
     * @param string $phpType The PHP type name
     * @return string The corresponding GraphQL type
     */
    private static function phpTypeToGraphQL(string $phpType): string
    {
        return match ($phpType) {
            'int' => 'Int',
            'string' => 'String',
            'bool' => 'Boolean',
            'float' => 'Float',
            'mixed' => 'String',
            'array' => 'String', // Default for untyped arrays
            default => $phpType, // Class names pass through (User -> User)
        };
    }

    /**
     * Get the complete GraphQL type string including list and nullable modifiers.
     *
     * Examples:
     * - Int! (non-nullable int)
     * - [String!]! (non-nullable list of non-nullable strings)
     * - [User] (nullable list of nullable User types)
     *
     * @param bool $nonNullList Whether the list itself should be non-nullable
     * @return string The complete GraphQL type string
     */
    public function toGraphQLTypeString(bool $nonNullList = false): string
    {
        $type = $this->graphQLType;

        if ($this->isList) {
            $type = '[' . $type . (!$this->isNullable ? '!' : '') . ']';
            if ($nonNullList) {
                $type .= '!';
            }
        } else {
            if (!$this->isNullable) {
                $type .= '!';
            }
        }

        return $type;
    }

    /**
     * Check if this type is a custom class type (not a built-in scalar).
     *
     * @return bool
     */
    public function isCustomType(): bool
    {
        return !in_array($this->phpType, ['int', 'string', 'bool', 'float', 'mixed', 'array'], true);
    }
}
