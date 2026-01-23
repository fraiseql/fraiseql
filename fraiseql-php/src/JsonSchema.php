<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Represents a GraphQL schema in JSON format for serialization and transmission.
 *
 * This class holds the complete schema definition including all types,
 * fields, and metadata in a format suitable for JSON serialization.
 */
final class JsonSchema
{
    /**
     * @param string $version Schema version
     * @param array<string, array> $types Type definitions indexed by type name
     * @param array<string, string> $scalars Scalar type definitions
     * @param string|null $description Optional schema description
     * @param array<string, mixed> $metadata Additional schema metadata
     * @param array<int, array<string, mixed>> $observers Observer definitions
     */
    public function __construct(
        public readonly string $version,
        public readonly array $types,
        public readonly array $scalars,
        public readonly ?string $description = null,
        public readonly array $metadata = [],
        public readonly array $observers = [],
    ) {
    }

    /**
     * Convert the schema to a JSON-serializable array.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $schema = [
            'version' => $this->version,
            'types' => $this->types,
            'scalars' => $this->scalars,
        ];

        if ($this->description !== null) {
            $schema['description'] = $this->description;
        }

        if (!empty($this->metadata)) {
            $schema['metadata'] = $this->metadata;
        }

        if (!empty($this->observers)) {
            $schema['observers'] = $this->observers;
        }

        return $schema;
    }

    /**
     * Convert the schema to JSON string.
     *
     * @param int $flags JSON_* flags for json_encode
     * @return string JSON representation of the schema
     */
    public function toJson(int $flags = JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES): string
    {
        $json = json_encode($this->toArray(), $flags);

        if ($json === false) {
            throw new FraiseQLException('Failed to encode schema to JSON: ' . json_last_error_msg());
        }

        return $json;
    }

    /**
     * Save the schema to a JSON file.
     *
     * @param string $filePath The file path to save to
     * @param int $flags JSON_* flags for json_encode
     * @return int The number of bytes written
     *
     * @throws FraiseQLException If file write fails
     */
    public function saveToFile(string $filePath, int $flags = JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES): int
    {
        $json = $this->toJson($flags);
        $bytes = file_put_contents($filePath, $json);

        if ($bytes === false) {
            throw new FraiseQLException("Failed to write schema to file: $filePath");
        }

        return $bytes;
    }

    /**
     * Load schema from a JSON file.
     *
     * @param string $filePath The file path to load from
     * @return self The loaded schema
     *
     * @throws FraiseQLException If file read or JSON parsing fails
     */
    public static function loadFromFile(string $filePath): self
    {
        if (!file_exists($filePath)) {
            throw new FraiseQLException("Schema file not found: $filePath");
        }

        $json = file_get_contents($filePath);

        if ($json === false) {
            throw new FraiseQLException("Failed to read schema file: $filePath");
        }

        return self::fromJson($json);
    }

    /**
     * Load schema from a JSON string.
     *
     * @param string $json The JSON string
     * @return self The loaded schema
     *
     * @throws FraiseQLException If JSON parsing fails
     */
    public static function fromJson(string $json): self
    {
        $data = json_decode($json, true);

        if ($data === null) {
            throw new FraiseQLException('Failed to decode JSON: ' . json_last_error_msg());
        }

        if (!is_array($data)) {
            throw new FraiseQLException('JSON must decode to an object/array');
        }

        return new self(
            version: $data['version'] ?? '1.0',
            types: $data['types'] ?? [],
            scalars: $data['scalars'] ?? [],
            description: $data['description'] ?? null,
            metadata: $data['metadata'] ?? [],
            observers: $data['observers'] ?? [],
        );
    }

    /**
     * Get all type names in the schema.
     *
     * @return array<string>
     */
    public function getTypeNames(): array
    {
        return array_keys($this->types);
    }

    /**
     * Get a specific type definition.
     *
     * @param string $typeName The type name
     * @return array<string, mixed>|null The type definition or null
     */
    public function getType(string $typeName): ?array
    {
        return $this->types[$typeName] ?? null;
    }

    /**
     * Check if a type exists in the schema.
     *
     * @param string $typeName The type name
     * @return bool
     */
    public function hasType(string $typeName): bool
    {
        return isset($this->types[$typeName]);
    }

    /**
     * Get the number of types in the schema.
     *
     * @return int
     */
    public function getTypeCount(): int
    {
        return count($this->types);
    }

    /**
     * Get all scalar type names.
     *
     * @return array<string>
     */
    public function getScalarNames(): array
    {
        return array_keys($this->scalars);
    }
}
