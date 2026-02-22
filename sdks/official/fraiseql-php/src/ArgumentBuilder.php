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

/**
 * Fluent builder for constructing GraphQL field arguments.
 *
 * Provides convenient methods for adding typed arguments to GraphQL fields.
 * All methods return $this for method chaining.
 *
 * Usage:
 * ```php
 * $args = ArgumentBuilder::new()
 *     ->argument('id', 'Int', nullable: false)
 *     ->argument('name', 'String', description: 'Filter by name')
 *     ->optionalArgument('limit', 'Int', defaultValue: 10);
 * ```
 */
final class ArgumentBuilder
{
    /** @var ArgumentDefinition[] */
    private array $arguments = [];

    /**
     * Create a new ArgumentBuilder instance.
     */
    public static function new(): self
    {
        return new self();
    }

    /**
     * Add an argument with full control over all parameters.
     *
     * @param string $name Argument name
     * @param string $type GraphQL type
     * @param bool $nullable Whether argument can be null
     * @param bool $isList Whether argument is a list
     * @param mixed $defaultValue Default value
     * @param string|null $description Argument description
     * @return self
     */
    public function argument(
        string $name,
        string $type,
        bool $nullable = true,
        bool $isList = false,
        mixed $defaultValue = null,
        ?string $description = null,
    ): self {
        $this->arguments[$name] = new ArgumentDefinition(
            name: $name,
            type: $type,
            nullable: $nullable,
            isList: $isList,
            defaultValue: $defaultValue,
            description: $description,
        );

        return $this;
    }

    /**
     * Add a required (non-nullable) scalar argument.
     *
     * @param string $name Argument name
     * @param string $type Scalar type (Int, String, Boolean, Float)
     * @param string|null $description Argument description
     * @return self
     */
    public function requiredArgument(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->argument($name, $type, nullable: false, description: $description);
    }

    /**
     * Add an optional (nullable) scalar argument.
     *
     * @param string $name Argument name
     * @param string $type Scalar type
     * @param mixed $defaultValue Default value if not provided
     * @param string|null $description Argument description
     * @return self
     */
    public function optionalArgument(
        string $name,
        string $type,
        mixed $defaultValue = null,
        ?string $description = null,
    ): self {
        return $this->argument(
            $name,
            $type,
            nullable: true,
            defaultValue: $defaultValue,
            description: $description,
        );
    }

    /**
     * Add a required list argument (non-nullable list of non-nullable items).
     *
     * @param string $name Argument name
     * @param string $type Item type
     * @param string|null $description Argument description
     * @return self
     */
    public function listArgument(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->argument(
            $name,
            $type,
            nullable: false,
            isList: true,
            description: $description,
        );
    }

    /**
     * Add an optional list argument (nullable list).
     *
     * @param string $name Argument name
     * @param string $type Item type
     * @param string|null $description Argument description
     * @return self
     */
    public function optionalListArgument(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->argument(
            $name,
            $type,
            nullable: true,
            isList: true,
            description: $description,
        );
    }

    /**
     * Add a required object type argument.
     *
     * @param string $name Argument name
     * @param string $typeName Custom type name
     * @param string|null $description Argument description
     * @return self
     */
    public function objectArgument(
        string $name,
        string $typeName,
        ?string $description = null,
    ): self {
        return $this->argument(
            $name,
            $typeName,
            nullable: false,
            description: $description,
        );
    }

    /**
     * Add an optional object type argument.
     *
     * @param string $name Argument name
     * @param string $typeName Custom type name
     * @param string|null $description Argument description
     * @return self
     */
    public function optionalObjectArgument(
        string $name,
        string $typeName,
        ?string $description = null,
    ): self {
        return $this->argument(
            $name,
            $typeName,
            nullable: true,
            description: $description,
        );
    }

    /**
     * Get all defined arguments.
     *
     * @return ArgumentDefinition[]
     */
    public function getArguments(): array
    {
        return $this->arguments;
    }

    /**
     * Get a specific argument by name.
     *
     * @param string $name Argument name
     * @return ArgumentDefinition|null The argument or null if not found
     */
    public function getArgument(string $name): ?ArgumentDefinition
    {
        return $this->arguments[$name] ?? null;
    }

    /**
     * Check if an argument exists.
     *
     * @param string $name Argument name
     * @return bool True if argument exists
     */
    public function hasArgument(string $name): bool
    {
        return isset($this->arguments[$name]);
    }

    /**
     * Get the count of defined arguments.
     *
     * @return int Number of arguments
     */
    public function getArgumentCount(): int
    {
        return count($this->arguments);
    }

    /**
     * Get argument names in definition order.
     *
     * @return string[]
     */
    public function getArgumentNames(): array
    {
        return array_keys($this->arguments);
    }

    /**
     * Convert arguments to array format for serialization.
     *
     * @return array<string, array> Arguments as associative array
     */
    public function toArray(): array
    {
        $result = [];

        foreach ($this->arguments as $name => $arg) {
            $result[$name] = [
                'type' => $arg->getGraphQLTypeString(),
                'nullable' => $arg->nullable,
                'description' => $arg->description,
            ];

            if ($arg->defaultValue !== null) {
                $result[$name]['defaultValue'] = $arg->defaultValue;
            }
        }

        return $result;
    }
}
