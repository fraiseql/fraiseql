<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Lazy loads type definitions on-demand to reduce initial memory overhead.
 *
 * Instead of registering all types eagerly, LazyLoader allows deferring
 * type loading until they are actually accessed, improving performance
 * for large schemas.
 */
final class LazyLoader
{
    /** @var array<string, callable> Loader callables indexed by type name */
    private array $loaders = [];

    /** @var array<string, bool> Loaded types cache */
    private array $loaded = [];

    /** @var SchemaRegistry Registry to load types into */
    private SchemaRegistry $registry;

    /**
     * Create a new LazyLoader for a registry.
     *
     * @param SchemaRegistry $registry Registry to load types into
     */
    public function __construct(SchemaRegistry $registry)
    {
        $this->registry = $registry;
    }

    /**
     * Register a lazy loader for a type.
     *
     * The callable should register the type with the registry when called.
     *
     * @param string $typeName Type name
     * @param callable(): void $loader Loader callable
     * @return void
     */
    public function registerLoader(string $typeName, callable $loader): void
    {
        $this->loaders[$typeName] = $loader;
        $this->loaded[$typeName] = false;
    }

    /**
     * Ensure a type is loaded.
     *
     * @param string $typeName Type name to load
     * @return bool True if type was loaded or was already loaded
     */
    public function ensureLoaded(string $typeName): bool
    {
        if ($this->loaded[$typeName] ?? false) {
            return true;
        }

        if (!isset($this->loaders[$typeName])) {
            return false;
        }

        $loader = $this->loaders[$typeName];
        $loader();
        $this->loaded[$typeName] = true;

        return true;
    }

    /**
     * Ensure all types are loaded.
     *
     * @return int Number of types loaded
     */
    public function ensureAllLoaded(): int
    {
        $count = 0;

        foreach (array_keys($this->loaders) as $typeName) {
            if (!isset($this->loaded[$typeName]) || !$this->loaded[$typeName]) {
                $this->ensureLoaded($typeName);
                $count++;
            }
        }

        return $count;
    }

    /**
     * Check if a type is registered with a loader.
     *
     * @param string $typeName Type name
     * @return bool
     */
    public function hasLoader(string $typeName): bool
    {
        return isset($this->loaders[$typeName]);
    }

    /**
     * Check if a type has been loaded.
     *
     * @param string $typeName Type name
     * @return bool
     */
    public function isLoaded(string $typeName): bool
    {
        return $this->loaded[$typeName] ?? false;
    }

    /**
     * Get all registered type names (loaded or not).
     *
     * @return string[]
     */
    public function getRegisteredTypes(): array
    {
        return array_keys($this->loaders);
    }

    /**
     * Get loaded type names.
     *
     * @return string[]
     */
    public function getLoadedTypes(): array
    {
        return array_keys(array_filter($this->loaded));
    }

    /**
     * Get unloaded type names.
     *
     * @return string[]
     */
    public function getUnloadedTypes(): array
    {
        $unloaded = [];

        foreach ($this->loaders as $typeName => $loader) {
            if (!($this->loaded[$typeName] ?? false)) {
                $unloaded[] = $typeName;
            }
        }

        return $unloaded;
    }

    /**
     * Get the number of registered types.
     *
     * @return int
     */
    public function getTypeCount(): int
    {
        return count($this->loaders);
    }

    /**
     * Get the number of loaded types.
     *
     * @return int
     */
    public function getLoadedCount(): int
    {
        return count(array_filter($this->loaded));
    }

    /**
     * Get the number of unloaded types.
     *
     * @return int
     */
    public function getUnloadedCount(): int
    {
        return $this->getTypeCount() - $this->getLoadedCount();
    }

    /**
     * Get loading progress as percentage.
     *
     * @return float Percentage between 0 and 100
     */
    public function getLoadingProgress(): float
    {
        if ($this->getTypeCount() === 0) {
            return 100.0;
        }

        return ($this->getLoadedCount() / $this->getTypeCount()) * 100;
    }

    /**
     * Clear all registrations and loaded state.
     *
     * @return void
     */
    public function clear(): void
    {
        $this->loaders = [];
        $this->loaded = [];
    }

    /**
     * Get statistics about loading state.
     *
     * @return array<string, mixed>
     */
    public function getStats(): array
    {
        return [
            'total_types' => $this->getTypeCount(),
            'loaded_types' => $this->getLoadedCount(),
            'unloaded_types' => $this->getUnloadedCount(),
            'loading_progress_percent' => $this->getLoadingProgress(),
        ];
    }
}
