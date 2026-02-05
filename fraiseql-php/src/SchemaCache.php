<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Caches compiled schema representations for improved performance.
 *
 * Stores formatted schemas, JSON exports, and validation results to avoid
 * recompilation during request lifecycles and between requests when appropriate.
 *
 * Features:
 * - Registry-aware caching
 * - TTL-based expiration
 * - Invalidation on schema changes
 * - Statistics tracking
 */
final class SchemaCache
{
    /** @var Cache In-memory cache storage */
    private Cache $cache;

    /** @var array<string, int> Cache entry timestamps */
    private array $timestamps = [];

    /** @var int|null Default TTL in seconds, null means no expiration */
    private ?int $ttl = null;

    /** @var int Cache hits */
    private int $hits = 0;

    /** @var int Cache misses */
    private int $misses = 0;

    /**
     * Create a new SchemaCache instance.
     *
     * @param int|null $ttl Time-to-live in seconds, null for no expiration
     */
    public function __construct(?int $ttl = null)
    {
        $this->cache = new Cache();
        $this->cache->setMaxEntries(500);
        $this->ttl = $ttl;
    }

    /**
     * Cache a formatted schema for a registry.
     *
     * @param SchemaRegistry $registry Registry being cached
     * @param JsonSchema $schema Formatted schema
     * @return void
     */
    public function cacheFormattedSchema(SchemaRegistry $registry, JsonSchema $schema): void
    {
        $key = CacheKey::forRegistry($registry);
        $this->cache->set($key, $schema);
        $this->timestamps[$key] = time();
    }

    /**
     * Retrieve a cached formatted schema.
     *
     * @param SchemaRegistry $registry Registry to retrieve for
     * @return JsonSchema|null Cached schema or null if not found/expired
     */
    public function getFormattedSchema(SchemaRegistry $registry): ?JsonSchema
    {
        $key = CacheKey::forRegistry($registry);

        if (!$this->cache->has($key)) {
            $this->misses++;
            return null;
        }

        if ($this->isExpired($key)) {
            $this->cache->delete($key);
            unset($this->timestamps[$key]);
            $this->misses++;
            return null;
        }

        $this->hits++;
        return $this->cache->get($key);
    }

    /**
     * Cache validation results for a registry.
     *
     * @param SchemaRegistry $registry Registry being validated
     * @param bool $isValid Whether registry is valid
     * @return void
     */
    public function cacheValidation(SchemaRegistry $registry, bool $isValid): void
    {
        $key = CacheKey::custom('validation', ['registry' => CacheKey::forRegistry($registry)]);
        $this->cache->set($key, $isValid);
        $this->timestamps[$key] = time();
    }

    /**
     * Retrieve cached validation result.
     *
     * @param SchemaRegistry $registry Registry to check
     * @return bool|null Cached result or null if not found/expired
     */
    public function getValidation(SchemaRegistry $registry): ?bool
    {
        $key = CacheKey::custom('validation', ['registry' => CacheKey::forRegistry($registry)]);

        if (!$this->cache->has($key)) {
            $this->misses++;
            return null;
        }

        if ($this->isExpired($key)) {
            $this->cache->delete($key);
            unset($this->timestamps[$key]);
            $this->misses++;
            return null;
        }

        $this->hits++;
        return $this->cache->get($key);
    }

    /**
     * Cache JSON representation of a schema.
     *
     * @param JsonSchema $schema Schema to cache
     * @param string $json JSON string
     * @return void
     */
    public function cacheJson(JsonSchema $schema, string $json): void
    {
        $key = CacheKey::custom('json', ['schema' => CacheKey::forJsonSchema($schema)]);
        $this->cache->set($key, $json);
        $this->timestamps[$key] = time();
    }

    /**
     * Retrieve cached JSON representation.
     *
     * @param JsonSchema $schema Schema to retrieve for
     * @return string|null Cached JSON or null if not found/expired
     */
    public function getJson(JsonSchema $schema): ?string
    {
        $key = CacheKey::custom('json', ['schema' => CacheKey::forJsonSchema($schema)]);

        if (!$this->cache->has($key)) {
            $this->misses++;
            return null;
        }

        if ($this->isExpired($key)) {
            $this->cache->delete($key);
            unset($this->timestamps[$key]);
            $this->misses++;
            return null;
        }

        $this->hits++;
        return $this->cache->get($key);
    }

    /**
     * Clear all cached schemas.
     *
     * @return void
     */
    public function clear(): void
    {
        $this->cache->clear();
        $this->timestamps = [];
    }

    /**
     * Check if a cache entry has expired.
     *
     * @param string $key Cache key to check
     * @return bool True if expired
     */
    private function isExpired(string $key): bool
    {
        if ($this->ttl === null) {
            return false;
        }

        $timestamp = $this->timestamps[$key] ?? 0;
        return (time() - $timestamp) > $this->ttl;
    }

    /**
     * Get cache hit count.
     *
     * @return int Number of cache hits
     */
    public function getHits(): int
    {
        return $this->hits;
    }

    /**
     * Get cache miss count.
     *
     * @return int Number of cache misses
     */
    public function getMisses(): int
    {
        return $this->misses;
    }

    /**
     * Get cache hit ratio.
     *
     * @return float Hit ratio between 0 and 1, 0 if no accesses yet
     */
    public function getHitRatio(): float
    {
        $total = $this->hits + $this->misses;
        return $total > 0 ? $this->hits / $total : 0.0;
    }

    /**
     * Get cache statistics.
     *
     * @return array<string, mixed>
     */
    public function getStats(): array
    {
        return [
            'hits' => $this->hits,
            'misses' => $this->misses,
            'hit_ratio' => $this->getHitRatio(),
            'entries' => $this->cache->count(),
            'ttl' => $this->ttl,
        ];
    }

    /**
     * Reset statistics counters.
     *
     * @return void
     */
    public function resetStats(): void
    {
        $this->hits = 0;
        $this->misses = 0;
    }
}
