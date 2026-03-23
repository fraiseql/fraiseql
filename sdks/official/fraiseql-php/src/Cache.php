<?php

declare(strict_types=1);

namespace FraiseQL;

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
