<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Fluent builder for constructing GraphQL type definitions programmatically.
 *
 * Allows manual type definition without relying on PHP attributes, useful for
 * types that cannot be annotated (third-party classes, dynamic types, etc.).
 *
 * Usage:
 * ```php
 * $builder = TypeBuilder::type('User')
 *     ->field('id', 'Int', nullable: false)
 *     ->field('name', 'String', nullable: false)
 *     ->field('email', 'String', nullable: true)
 *     ->description('A user in the system');
 *
 * $registry = SchemaRegistry::getInstance();
 * $registry->registerBuilder($builder);
 * ```
 */
final class TypeBuilder
{
    /** @var array<string, FieldDefinition> */
    private array $fields = [];

    private ?string $description = null;

    /**
     * Create a new TypeBuilder for a type.
     *
     * @param string $name The GraphQL type name
     * @return self
     */
    public static function type(string $name): self
    {
        return new self($name);
    }

    /**
     * @param string $name The GraphQL type name
     */
    private function __construct(private readonly string $name)
    {
    }

    /**
     * Add a field to the type.
     *
     * @param string $name The field name
     * @param string $type The GraphQL type (Int, String, User, etc.)
     * @param bool $nullable Whether the field is nullable
     * @param bool $isList Whether the field is a list
     * @param string|null $description Optional field description
     * @param string|null $customResolver Optional resolver method name
     * @return self Fluent interface
     */
    public function field(
        string $name,
        string $type,
        bool $nullable = false,
        bool $isList = false,
        ?string $description = null,
        ?string $customResolver = null,
    ): self {
        $this->fields[$name] = new FieldDefinition(
            name: $name,
            type: $type,
            nullable: $nullable,
            isList: $isList,
            description: $description,
            phpType: 'mixed',
            customResolver: $customResolver,
            parentType: $this->name,
        );

        return $this;
    }

    /**
     * Add a non-nullable scalar field.
     *
     * @param string $name The field name
     * @param string $type The scalar type (Int, String, Boolean, Float)
     * @param string|null $description Optional field description
     * @return self Fluent interface
     */
    public function scalarField(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->field($name, $type, nullable: false, description: $description);
    }

    /**
     * Add a nullable scalar field.
     *
     * @param string $name The field name
     * @param string $type The scalar type (Int, String, Boolean, Float)
     * @param string|null $description Optional field description
     * @return self Fluent interface
     */
    public function optionalField(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->field($name, $type, nullable: true, description: $description);
    }

    /**
     * Add a non-nullable list field.
     *
     * @param string $name The field name
     * @param string $type The item type
     * @param string|null $description Optional field description
     * @return self Fluent interface
     */
    public function listField(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->field($name, $type, nullable: false, isList: true, description: $description);
    }

    /**
     * Add a nullable list field.
     *
     * @param string $name The field name
     * @param string $type The item type
     * @param string|null $description Optional field description
     * @return self Fluent interface
     */
    public function optionalListField(
        string $name,
        string $type,
        ?string $description = null,
    ): self {
        return $this->field($name, $type, nullable: true, isList: true, description: $description);
    }

    /**
     * Add a custom resolver to the last added field.
     *
     * @param string $fieldName The field to add resolver to
     * @param string $methodName The resolver method name
     * @return self Fluent interface
     */
    public function withResolver(string $fieldName, string $methodName): self
    {
        if (isset($this->fields[$fieldName])) {
            $field = $this->fields[$fieldName];
            $this->fields[$fieldName] = new FieldDefinition(
                name: $field->name,
                type: $field->type,
                nullable: $field->nullable,
                isList: $field->isList,
                description: $field->description,
                phpType: $field->phpType,
                customResolver: $methodName,
                parentType: $field->parentType,
            );
        }

        return $this;
    }

    /**
     * Set the type description.
     *
     * @param string $description The description
     * @return self Fluent interface
     */
    public function description(string $description): self
    {
        $this->description = $description;

        return $this;
    }

    /**
     * Get the type name.
     *
     * @return string
     */
    public function getName(): string
    {
        return $this->name;
    }

    /**
     * Get the type description.
     *
     * @return string|null
     */
    public function getDescription(): ?string
    {
        return $this->description;
    }

    /**
     * Get all fields.
     *
     * @return array<string, FieldDefinition>
     */
    public function getFields(): array
    {
        return $this->fields;
    }

    /**
     * Get a specific field.
     *
     * @param string $name The field name
     * @return FieldDefinition|null
     */
    public function getField(string $name): ?FieldDefinition
    {
        return $this->fields[$name] ?? null;
    }

    /**
     * Check if a field exists.
     *
     * @param string $name The field name
     * @return bool
     */
    public function hasField(string $name): bool
    {
        return isset($this->fields[$name]);
    }

    /**
     * Get field count.
     *
     * @return int
     */
    public function getFieldCount(): int
    {
        return count($this->fields);
    }
}
