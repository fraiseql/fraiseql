<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Represents a GraphQL argument definition with type and validation information.
 *
 * Arguments are parameters passed to fields or directives in GraphQL queries.
 * This class provides immutable storage for argument metadata.
 */
final class ArgumentDefinition
{
    /**
     * @param string $name Argument name
     * @param string $type GraphQL type (e.g., 'String', 'Int', 'User')
     * @param bool $nullable Whether argument can be null
     * @param bool $isList Whether argument is a list type
     * @param mixed $defaultValue Default value if not provided
     * @param string|null $description Human-readable description
     */
    public function __construct(
        public readonly string $name,
        public readonly string $type,
        public readonly bool $nullable = true,
        public readonly bool $isList = false,
        public readonly mixed $defaultValue = null,
        public readonly ?string $description = null,
    ) {
    }

    /**
     * Get the GraphQL type string with modifiers.
     *
     * @param bool $nonNullList If true, wraps list in ! (only applies if isList=true)
     * @return string GraphQL type string (e.g., 'String!', '[Int!]', '[User]!')
     */
    public function getGraphQLTypeString(bool $nonNullList = false): string
    {
        $baseType = $this->type;

        if ($this->isList) {
            $itemType = $this->nullable ? $baseType : $baseType . '!';
            $listType = '[' . $itemType . ']';
            return $nonNullList ? $listType . '!' : $listType;
        }

        return $this->nullable ? $baseType : $baseType . '!';
    }

    /**
     * Check if argument is a scalar type.
     *
     * @return bool True if type is a GraphQL scalar (Int, String, Boolean, Float)
     */
    public function isScalar(): bool
    {
        return in_array($this->type, ['Int', 'String', 'Boolean', 'Float'], true);
    }

    /**
     * Get string representation for debugging.
     *
     * @return string Human-readable argument definition
     */
    public function __toString(): string
    {
        $type = $this->getGraphQLTypeString();
        $default = $this->defaultValue !== null ? ' = ' . json_encode($this->defaultValue) : '';
        return "{$this->name}: {$type}{$default}";
    }
}
