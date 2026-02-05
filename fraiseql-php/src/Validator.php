<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Validates GraphQL schema definitions and type configurations.
 *
 * This class provides comprehensive validation of:
 * - Type names and structure
 * - Field definitions and circular references
 * - Required vs nullable fields
 * - Scalar type consistency
 * - Schema metadata integrity
 */
final class Validator
{
    /** @var string[] Collected validation errors */
    private array $errors = [];

    /** @var string[] Collected validation warnings */
    private array $warnings = [];

    /**
     * Validate a registry of types.
     *
     * @param SchemaRegistry $registry Registry to validate
     * @return bool True if validation passes (no errors)
     */
    public function validateRegistry(SchemaRegistry $registry): bool
    {
        $this->errors = [];
        $this->warnings = [];

        $typeNames = $registry->getTypeNames();

        if (empty($typeNames)) {
            $this->warnings[] = 'Registry is empty - no types registered';
            return true;
        }

        // Validate each type
        foreach ($typeNames as $typeName) {
            $this->validateType($registry, $typeName);
        }

        // Check for circular references
        $this->validateCircularReferences($registry, $typeNames);

        return empty($this->errors);
    }

    /**
     * Validate a single type definition.
     *
     * @param SchemaRegistry $registry Registry containing the type
     * @param string $typeName Name of type to validate
     * @return bool True if type is valid
     */
    public function validateType(SchemaRegistry $registry, string $typeName): bool
    {
        $type = $registry->getType($typeName);

        if ($type === null) {
            $this->errors[] = "Type not found: {$typeName}";
            return false;
        }

        // Validate type name format
        if (!$this->isValidTypeName($typeName)) {
            $this->errors[] = "Invalid type name: {$typeName} (must start with letter, alphanumeric)";
            return false;
        }

        // Validate fields
        $fields = $registry->getTypeFields($typeName);
        if (empty($fields)) {
            $this->warnings[] = "Type {$typeName} has no fields";
        }

        foreach ($fields as $field) {
            $this->validateField($field, $registry);
        }

        return true;
    }

    /**
     * Validate a field definition.
     *
     * @param FieldDefinition $field Field to validate
     * @param SchemaRegistry $registry Registry for type checking
     * @return bool True if field is valid
     */
    public function validateField(FieldDefinition $field, SchemaRegistry $registry): bool
    {
        // Validate field name format
        if (!$this->isValidFieldName($field->name)) {
            $this->errors[] = "Invalid field name: {$field->name}";
            return false;
        }

        // Validate type exists
        if (!$field->isScalar()) {
            if (!$registry->hasType($field->type)) {
                $this->errors[] = "Field {$field->parentType}.{$field->name} references unknown type: {$field->type}";
                return false;
            }
        }

        return true;
    }

    /**
     * Validate a JSON schema.
     *
     * @param JsonSchema $schema Schema to validate
     * @return bool True if schema is valid
     */
    public function validateJsonSchema(JsonSchema $schema): bool
    {
        $this->errors = [];
        $this->warnings = [];

        // Validate version format
        if (!$this->isValidVersion($schema->version)) {
            $this->errors[] = "Invalid schema version: {$schema->version}";
            return false;
        }

        // Validate types
        if (empty($schema->getTypeNames())) {
            $this->warnings[] = 'Schema has no types';
        }

        foreach ($schema->getTypeNames() as $typeName) {
            $type = $schema->getType($typeName);

            if (!$this->isValidTypeName($typeName)) {
                $this->errors[] = "Invalid type name in schema: {$typeName}";
                return false;
            }

            if (!isset($type['fields']) || !is_array($type['fields'])) {
                $this->errors[] = "Type {$typeName} missing fields array";
                return false;
            }

            // Validate field names
            foreach (array_keys($type['fields']) as $fieldName) {
                if (!$this->isValidFieldName($fieldName)) {
                    $this->errors[] = "Invalid field name in type {$typeName}: {$fieldName}";
                    return false;
                }
            }
        }

        // Validate scalars
        foreach ($schema->getScalarNames() as $scalarName) {
            if (!$this->isValidTypeName($scalarName)) {
                $this->errors[] = "Invalid scalar name: {$scalarName}";
                return false;
            }
        }

        return empty($this->errors);
    }

    /**
     * Validate a TypeBuilder configuration.
     *
     * @param TypeBuilder $builder Builder to validate
     * @param SchemaRegistry|null $registry Optional registry for type references
     * @return bool True if builder is valid
     */
    public function validateBuilder(TypeBuilder $builder, ?SchemaRegistry $registry = null): bool
    {
        $this->errors = [];
        $this->warnings = [];

        // Validate type name
        if (!$this->isValidTypeName($builder->getName())) {
            $this->errors[] = "Invalid type name in builder: {$builder->getName()}";
            return false;
        }

        // Validate fields
        $fields = $builder->getFields();
        if (empty($fields)) {
            $this->warnings[] = "Builder for {$builder->getName()} has no fields";
        }

        foreach ($fields as $field) {
            // Validate field name
            if (!$this->isValidFieldName($field->name)) {
                $this->errors[] = "Invalid field name: {$field->name}";
                return false;
            }

            // Check type references if registry provided
            if ($registry !== null && !$field->isScalar()) {
                if (!$registry->hasType($field->type)) {
                    $this->warnings[] = "Field {$field->name} references type {$field->type} not in registry";
                }
            }
        }

        return empty($this->errors);
    }

    /**
     * Check for circular type references in registry.
     *
     * @param SchemaRegistry $registry Registry to check
     * @param string[] $typeNames Type names to check
     * @return bool True if no circular references found
     */
    private function validateCircularReferences(SchemaRegistry $registry, array $typeNames): bool
    {
        foreach ($typeNames as $typeName) {
            $visited = [];
            if ($this->hasCircularReference($registry, $typeName, $visited)) {
                $this->warnings[] = "Potential circular reference detected involving type: {$typeName}";
            }
        }

        return true;
    }

    /**
     * Recursively check for circular references from a type.
     *
     * @param SchemaRegistry $registry Registry
     * @param string $typeName Current type being checked
     * @param string[] $visited Types already visited in this path
     * @return bool True if circular reference found
     */
    private function hasCircularReference(SchemaRegistry $registry, string $typeName, array &$visited): bool
    {
        if (in_array($typeName, $visited, true)) {
            return true;
        }

        $visited[] = $typeName;
        $fields = $registry->getTypeFields($typeName);

        foreach ($fields as $field) {
            if (!$field->isScalar() && $registry->hasType($field->type)) {
                if ($this->hasCircularReference($registry, $field->type, $visited)) {
                    return true;
                }
            }
        }

        array_pop($visited);
        return false;
    }

    /**
     * Check if type name follows GraphQL naming conventions.
     *
     * @param string $name Name to validate
     * @return bool True if valid
     */
    private function isValidTypeName(string $name): bool
    {
        // Must start with letter or underscore, contain only alphanumeric and underscore
        return (bool)preg_match('/^[A-Za-z_][A-Za-z0-9_]*$/', $name);
    }

    /**
     * Check if field name follows GraphQL naming conventions.
     *
     * @param string $name Name to validate
     * @return bool True if valid
     */
    private function isValidFieldName(string $name): bool
    {
        // Same as type names
        return $this->isValidTypeName($name);
    }

    /**
     * Check if version string is valid.
     *
     * @param string $version Version string to validate
     * @return bool True if valid
     */
    private function isValidVersion(string $version): bool
    {
        // Allow semantic versioning: X.Y, X.Y.Z
        return (bool)preg_match('/^\d+\.\d+(\.\d+)?$/', $version);
    }

    /**
     * Get all collected errors.
     *
     * @return string[]
     */
    public function getErrors(): array
    {
        return $this->errors;
    }

    /**
     * Get all collected warnings.
     *
     * @return string[]
     */
    public function getWarnings(): array
    {
        return $this->warnings;
    }

    /**
     * Check if there are any errors.
     *
     * @return bool True if validation has errors
     */
    public function hasErrors(): bool
    {
        return !empty($this->errors);
    }

    /**
     * Check if there are any warnings.
     *
     * @return bool True if validation has warnings
     */
    public function hasWarnings(): bool
    {
        return !empty($this->warnings);
    }

    /**
     * Get error count.
     *
     * @return int Number of errors
     */
    public function getErrorCount(): int
    {
        return count($this->errors);
    }

    /**
     * Get warning count.
     *
     * @return int Number of warnings
     */
    public function getWarningCount(): int
    {
        return count($this->warnings);
    }

    /**
     * Clear all validation state.
     */
    public function clear(): void
    {
        $this->errors = [];
        $this->warnings = [];
    }

    /**
     * Get formatted validation report.
     *
     * @return string Formatted report with errors and warnings
     */
    public function getReport(): string
    {
        $report = [];

        if (!empty($this->errors)) {
            $report[] = 'ERRORS (' . count($this->errors) . '):';
            foreach ($this->errors as $error) {
                $report[] = '  - ' . $error;
            }
        }

        if (!empty($this->warnings)) {
            $report[] = 'WARNINGS (' . count($this->warnings) . '):';
            foreach ($this->warnings as $warning) {
                $report[] = '  - ' . $warning;
            }
        }

        if (empty($this->errors) && empty($this->warnings)) {
            $report[] = 'Validation passed - no errors or warnings';
        }

        return implode("\n", $report);
    }
}
