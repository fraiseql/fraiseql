<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Generates cache keys for FraiseQL operations.
 *
 * This class provides consistent cache key generation for:
 * - Schema compilations
 * - Type definitions
 * - Field resolutions
 *
 * Cache keys are deterministic and version-aware.
 */
final class CacheKey
{
    private const VERSION = '1.0.0';

    /**
     * Generate cache key for a schema registry.
     *
     * @param SchemaRegistry $registry Registry to generate key for
     * @return string Unique cache key
     */
    public static function forRegistry(SchemaRegistry $registry): string
    {
        $typeNames = $registry->getTypeNames();
        sort($typeNames);

        $data = [
            'version' => self::VERSION,
            'type' => 'registry',
            'types' => $typeNames,
            'typeCount' => count($typeNames),
        ];

        return self::hash($data);
    }

    /**
     * Generate cache key for a JSON schema.
     *
     * @param JsonSchema $schema Schema to generate key for
     * @return string Unique cache key
     */
    public static function forJsonSchema(JsonSchema $schema): string
    {
        $data = [
            'version' => $schema->version,
            'type' => 'jsonschema',
            'typeCount' => $schema->getTypeCount(),
            'scalarCount' => count($schema->getScalarNames()),
            'description' => $schema->description ?? '',
        ];

        return self::hash($data);
    }

    /**
     * Generate cache key for a type builder.
     *
     * @param TypeBuilder $builder Builder to generate key for
     * @return string Unique cache key
     */
    public static function forBuilder(TypeBuilder $builder): string
    {
        $fields = $builder->getFields();
        $fieldNames = array_map(static fn (FieldDefinition $f) => $f->name, $fields);
        sort($fieldNames);

        $data = [
            'version' => self::VERSION,
            'type' => 'builder',
            'typeName' => $builder->getName(),
            'fieldCount' => count($fieldNames),
            'fieldNames' => $fieldNames,
            'description' => $builder->getDescription() ?? '',
        ];

        return self::hash($data);
    }

    /**
     * Generate cache key for a type name.
     *
     * @param string $typeName Name of type
     * @return string Unique cache key
     */
    public static function forType(string $typeName): string
    {
        $data = [
            'version' => self::VERSION,
            'type' => 'type',
            'typeName' => $typeName,
        ];

        return self::hash($data);
    }

    /**
     * Generate cache key for field resolution.
     *
     * @param string $typeName Type containing field
     * @param string $fieldName Field name
     * @return string Unique cache key
     */
    public static function forField(string $typeName, string $fieldName): string
    {
        $data = [
            'version' => self::VERSION,
            'type' => 'field',
            'typeName' => $typeName,
            'fieldName' => $fieldName,
        ];

        return self::hash($data);
    }

    /**
     * Generate cache key for formatted output.
     *
     * @param string $format Output format (e.g., 'json', 'sdl')
     * @param array<string, mixed> $options Format options
     * @return string Unique cache key
     */
    public static function forFormat(string $format, array $options = []): string
    {
        ksort($options);

        $data = [
            'version' => self::VERSION,
            'type' => 'format',
            'format' => $format,
            'options' => $options,
        ];

        return self::hash($data);
    }

    /**
     * Generate cache key from custom data.
     *
     * @param string $namespace Cache namespace/category
     * @param array<string, mixed> $data Data to key on
     * @return string Unique cache key
     */
    public static function custom(string $namespace, array $data): string
    {
        ksort($data);

        $payload = [
            'version' => self::VERSION,
            'namespace' => $namespace,
            'data' => $data,
        ];

        return self::hash($payload);
    }

    /**
     * Hash data into a cache key.
     *
     * @param array<string, mixed> $data Data to hash
     * @return string Hashed cache key
     */
    private static function hash(array $data): string
    {
        ksort($data);
        $json = json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);

        return 'fraiseql_' . hash('sha256', $json);
    }
}
