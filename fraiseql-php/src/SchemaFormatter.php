<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Formats GraphQL schemas into JSON representation for export and compilation.
 *
 * This class transforms type definitions from the SchemaRegistry into a
 * standardized JSON format suitable for transmission or storage.
 *
 * Usage:
 * ```php
 * $registry = SchemaRegistry::getInstance();
 * $registry->register(User::class);
 * $registry->register(Product::class);
 *
 * $formatter = new SchemaFormatter();
 * $schema = $formatter->formatRegistry($registry);
 * $json = $schema->toJson();
 * ```
 */
final class SchemaFormatter
{
    private const SCHEMA_VERSION = '1.0';

    private const BUILTIN_SCALARS = [
        'Int' => 'Integer scalar type',
        'String' => 'String scalar type',
        'Boolean' => 'Boolean scalar type',
        'Float' => 'Floating point scalar type',
    ];

    /**
     * Format all types from a SchemaRegistry into a JsonSchema.
     *
     * @param SchemaRegistry $registry The registry to format
     * @param string|null $description Optional schema description
     * @return JsonSchema The formatted schema
     */
    public function formatRegistry(
        SchemaRegistry $registry,
        ?string $description = null,
    ): JsonSchema {
        $types = [];
        $usedScalars = [];

        foreach ($registry->getTypeNames() as $typeName) {
            $types[$typeName] = $this->formatType($registry, $typeName, $usedScalars);
        }

        return new JsonSchema(
            version: self::SCHEMA_VERSION,
            types: $types,
            scalars: $this->formatScalars($usedScalars),
            description: $description,
        );
    }

    /**
     * Format a single type from the registry.
     *
     * @param SchemaRegistry $registry The registry
     * @param string $typeName The type name to format
     * @param array<string> $usedScalars Reference array to track used scalars
     * @return array<string, mixed> The formatted type definition
     */
    private function formatType(
        SchemaRegistry $registry,
        string $typeName,
        array &$usedScalars,
    ): array {
        $typeData = ['name' => $typeName];

        $type = $registry->getType($typeName);
        if ($type !== null) {
            if ($type->description !== null) {
                $typeData['description'] = $type->description;
            }
        }

        $fields = $registry->getTypeFields($typeName);
        $formattedFields = [];

        foreach ($fields as $field) {
            $formattedFields[$field->name] = $this->formatField($field, $usedScalars);
        }

        $typeData['fields'] = $formattedFields;

        return $typeData;
    }

    /**
     * Format a single field definition.
     *
     * @param FieldDefinition $field The field to format
     * @param array<string> $usedScalars Reference array to track used scalars
     * @return array<string, mixed> The formatted field definition
     */
    private function formatField(FieldDefinition $field, array &$usedScalars): array
    {
        $fieldData = [
            'type' => $field->getGraphQLTypeString(),
        ];

        // Track used scalars
        if (in_array($field->type, array_keys(self::BUILTIN_SCALARS), true)) {
            $usedScalars[$field->type] = true;
        }

        if ($field->description !== null) {
            $fieldData['description'] = $field->description;
        }

        if ($field->hasCustomResolver()) {
            $fieldData['resolver'] = $field->customResolver;
        }

        if ($field->phpType !== 'mixed') {
            $fieldData['phpType'] = $field->phpType;
        }

        return $fieldData;
    }

    /**
     * Format scalar type definitions.
     *
     * @param array<string, bool> $usedScalars Scalars to format (keys only)
     * @return array<string, string> Formatted scalars
     */
    private function formatScalars(array $usedScalars): array
    {
        $scalars = [];

        foreach (array_keys($usedScalars) as $scalarName) {
            if (isset(self::BUILTIN_SCALARS[$scalarName])) {
                $scalars[$scalarName] = self::BUILTIN_SCALARS[$scalarName];
            }
        }

        return $scalars;
    }

    /**
     * Format a TypeBuilder into a JsonSchema with a single type.
     *
     * @param TypeBuilder $builder The builder to format
     * @return JsonSchema The formatted schema with single type
     */
    public function formatBuilder(TypeBuilder $builder): JsonSchema
    {
        $type = [
            'name' => $builder->getName(),
            'fields' => [],
        ];

        if ($builder->getDescription() !== null) {
            $type['description'] = $builder->getDescription();
        }

        $usedScalars = [];

        foreach ($builder->getFields() as $field) {
            $type['fields'][$field->name] = $this->formatField($field, $usedScalars);
        }

        return new JsonSchema(
            version: self::SCHEMA_VERSION,
            types: [$builder->getName() => $type],
            scalars: $this->formatScalars($usedScalars),
        );
    }

    /**
     * Format multiple TypeBuilders into a JsonSchema.
     *
     * @param TypeBuilder ...$builders The builders to format
     * @return JsonSchema The formatted schema
     */
    public function formatBuilders(TypeBuilder ...$builders): JsonSchema
    {
        $types = [];
        $usedScalars = [];

        foreach ($builders as $builder) {
            $type = [
                'name' => $builder->getName(),
                'fields' => [],
            ];

            if ($builder->getDescription() !== null) {
                $type['description'] = $builder->getDescription();
            }

            foreach ($builder->getFields() as $field) {
                $type['fields'][$field->name] = $this->formatField($field, $usedScalars);
            }

            $types[$builder->getName()] = $type;
        }

        return new JsonSchema(
            version: self::SCHEMA_VERSION,
            types: $types,
            scalars: $this->formatScalars($usedScalars),
        );
    }
}
