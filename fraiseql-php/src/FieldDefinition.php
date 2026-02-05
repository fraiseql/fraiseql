<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Represents a GraphQL field definition with metadata and type information.
 *
 * This class holds all information about a field including its GraphQL type,
 * nullability, description, and optional resolver function.
 */
final readonly class FieldDefinition
{
    /**
     * @param string $name The field name
     * @param string $type The GraphQL type (Int, String, User, etc.)
     * @param bool $nullable Whether the field is nullable in GraphQL
     * @param bool $isList Whether the field is a list type
     * @param string|null $description Optional field description
     * @param string $phpType The original PHP type
     * @param string|null $customResolver Optional custom resolver method name
     * @param string $parentType The parent GraphQL type name
     * @param string|null $scope Optional JWT scope required to access this field
     * @param array<string>|null $scopes Optional JWT scopes required to access this field
     */
    public function __construct(
        public string $name,
        public string $type,
        public bool $nullable = false,
        public bool $isList = false,
        public ?string $description = null,
        public string $phpType = 'mixed',
        public ?string $customResolver = null,
        public string $parentType = 'Unknown',
        public ?string $scope = null,
        public ?array $scopes = null,
    ) {
    }

    /**
     * Get the complete GraphQL type string with modifiers.
     *
     * Examples:
     * - "Int!" (non-nullable int)
     * - "[String!]!" (non-nullable list of non-nullable strings)
     * - "[User]" (nullable list of nullable User types)
     *
     * @param bool $nonNullList Whether the list itself should be non-nullable
     * @return string The GraphQL type string
     */
    public function getGraphQLTypeString(bool $nonNullList = false): string
    {
        $type = $this->type;

        if ($this->isList) {
            $type = '[' . $type . (!$this->nullable ? '!' : '') . ']';
            if ($nonNullList) {
                $type .= '!';
            }
        } else {
            if (!$this->nullable) {
                $type .= '!';
            }
        }

        return $type;
    }

    /**
     * Check if this field is a scalar type.
     *
     * @return bool
     */
    public function isScalar(): bool
    {
        return in_array($this->type, ['Int', 'String', 'Boolean', 'Float'], true);
    }

    /**
     * Check if this field has a custom resolver.
     *
     * @return bool
     */
    public function hasCustomResolver(): bool
    {
        return $this->customResolver !== null;
    }

    /**
     * Get the single scope requirement for this field if present.
     *
     * @return string|null
     */
    public function getScope(): ?string
    {
        return $this->scope;
    }

    /**
     * Get the multiple scope requirements for this field if present.
     *
     * @return array<string>|null
     */
    public function getScopes(): ?array
    {
        return $this->scopes;
    }

    /**
     * Check if this field has any scope requirement.
     *
     * @return bool
     */
    public function hasScope(): bool
    {
        return $this->scope !== null || $this->scopes !== null;
    }

    /**
     * Get a string representation of the field for debugging.
     *
     * @return string
     */
    public function __toString(): string
    {
        return "{$this->parentType}.{$this->name}: {$this->getGraphQLTypeString()}";
    }
}
