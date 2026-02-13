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
        $fieldNames = array_map(static fn(FieldDefinition $f) => $f->name, $fields);
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
        $json = json_encode($data, JSON_SORT_KEYS | JSON_UNESCAPED_SLASHES);

        if ($json === false) {
            throw new FraiseQLException('Failed to JSON encode cache key data');
        }

        return 'fraiseql_' . hash('sha256', $json);
    }
}

/**
 * Simple in-memory cache for FraiseQL operations.
 *
 * Stores computed results during a request lifecycle.
 * Thread-safe for PHP's single-threaded model.
 */
final class Cache
{
    /** @var array<string, mixed> In-memory cache storage */
    private array $storage = [];

    /** @var int Maximum cache entries */
    private int $maxEntries = 1000;

    /** @var int Current entry count */
    private int $entryCount = 0;

    /**
     * Store a value in cache.
     *
     * @param string $key Cache key
     * @param mixed $value Value to cache
     * @return void
     */
    public function set(string $key, mixed $value): void
    {
        // Simple LRU: remove oldest entry if at capacity
        if ($this->entryCount >= $this->maxEntries && !isset($this->storage[$key])) {
            // Remove first entry
            $firstKey = array_key_first($this->storage);
            if ($firstKey !== null) {
                unset($this->storage[$firstKey]);
                $this->entryCount--;
            }
        }

        if (!isset($this->storage[$key])) {
            $this->entryCount++;
        }

        $this->storage[$key] = $value;
    }

    /**
     * Retrieve a value from cache.
     *
     * @param string $key Cache key
     * @return mixed|null Value or null if not found
     */
    public function get(string $key): mixed
    {
        return $this->storage[$key] ?? null;
    }

    /**
     * Check if key exists in cache.
     *
     * @param string $key Cache key
     * @return bool True if key exists
     */
    public function has(string $key): bool
    {
        return isset($this->storage[$key]);
    }

    /**
     * Remove a value from cache.
     *
     * @param string $key Cache key
     * @return bool True if key existed and was removed
     */
    public function delete(string $key): bool
    {
        if (isset($this->storage[$key])) {
            unset($this->storage[$key]);
            $this->entryCount--;
            return true;
        }

        return false;
    }

    /**
     * Clear all cache entries.
     *
     * @return void
     */
    public function clear(): void
    {
        $this->storage = [];
        $this->entryCount = 0;
    }

    /**
     * Get number of cached entries.
     *
     * @return int Entry count
     */
    public function count(): int
    {
        return $this->entryCount;
    }

    /**
     * Set maximum cache entries.
     *
     * @param int $max Maximum entries
     * @return void
     */
    public function setMaxEntries(int $max): void
    {
        $this->maxEntries = $max;
    }

    /**
     * Get all cached keys.
     *
     * @return string[]
     */
    public function keys(): array
    {
        return array_keys($this->storage);
    }

    /**
     * Get cache statistics.
     *
     * @return array<string, int>
     */
    public function getStats(): array
    {
        return [
            'entries' => $this->entryCount,
            'max_entries' => $this->maxEntries,
            'usage_percent' => $this->maxEntries > 0 ? (int)(($this->entryCount / $this->maxEntries) * 100) : 0,
        ];
    }
}
