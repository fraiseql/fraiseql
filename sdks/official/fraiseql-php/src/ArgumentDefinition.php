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
     * The GraphQL type string: `String!`, `[Int!]!`, `[ID!]` (a nullable list of non-null
     * items); see {@see TypeInfo::graphQLTypeString()}.
     */
    public function getGraphQLTypeString(): string
    {
        return TypeInfo::graphQLTypeString($this->type, $this->isList, $this->nullable);
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
