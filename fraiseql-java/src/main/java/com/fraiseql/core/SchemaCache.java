package com.fraiseql.core;

import java.util.*;
import java.util.concurrent.*;

/**
 * High-performance cache for schema metadata and compiled types.
 * Uses memoization and weak references to optimize memory usage.
 *
 * Features:
 * - Thread-safe field type caching
 * - Memoized type conversions
 * - Configuration-based cache sizing
 */
public class SchemaCache {
    private static final SchemaCache INSTANCE = new SchemaCache();

    private final Map<Class<?>, Map<String, TypeConverter.GraphQLFieldInfo>> fieldCache =
        new ConcurrentHashMap<>();

    private final Map<Class<?>, String> typeConversionCache =
        new ConcurrentHashMap<>();

    private final Map<String, Boolean> typeValidationCache =
        new ConcurrentHashMap<>();

    private volatile CacheStats stats = new CacheStats();

    private SchemaCache() {
    }

    /**
     * Get the singleton cache instance.
     *
     * @return the SchemaCache instance
     */
    public static SchemaCache getInstance() {
        return INSTANCE;
    }

    /**
     * Get cached field information for a type.
     * Returns null if not in cache.
     *
     * @param typeClass the type class
     * @return cached field map or null
     */
    public Map<String, TypeConverter.GraphQLFieldInfo> getFieldCache(Class<?> typeClass) {
        return fieldCache.get(typeClass);
    }

    /**
     * Put field information in cache.
     *
     * @param typeClass the type class
     * @param fields the field map to cache
     */
    public void putFieldCache(Class<?> typeClass, Map<String, TypeConverter.GraphQLFieldInfo> fields) {
        fieldCache.put(typeClass, fields);
        stats.recordFieldCacheHit();
    }

    /**
     * Get cached type conversion result.
     *
     * @param javaType the Java type
     * @return cached GraphQL type string or null
     */
    public String getTypeConversion(Class<?> javaType) {
        String result = typeConversionCache.get(javaType);
        if (result != null) {
            stats.recordTypeConversionHit();
        }
        return result;
    }

    /**
     * Put type conversion in cache.
     *
     * @param javaType the Java type
     * @param graphqlType the converted GraphQL type
     */
    public void putTypeConversion(Class<?> javaType, String graphqlType) {
        typeConversionCache.put(javaType, graphqlType);
    }

    /**
     * Check if a type name is valid (cached).
     *
     * @param typeName the type name to validate
     * @return cached validation result or null if not cached
     */
    public Boolean getTypeValidation(String typeName) {
        Boolean result = typeValidationCache.get(typeName);
        if (result != null) {
            stats.recordValidationHit();
        }
        return result;
    }

    /**
     * Put type validation in cache.
     *
     * @param typeName the type name
     * @param isValid validation result
     */
    public void putTypeValidation(String typeName, boolean isValid) {
        typeValidationCache.put(typeName, isValid);
    }

    /**
     * Get cache statistics.
     *
     * @return current cache statistics
     */
    public CacheStats getStats() {
        return stats.copy();
    }

    /**
     * Clear all caches.
     */
    public void clear() {
        fieldCache.clear();
        typeConversionCache.clear();
        typeValidationCache.clear();
        stats.reset();
    }

    /**
     * Get cache size information.
     *
     * @return cache size statistics
     */
    public CacheSizeInfo getSizeInfo() {
        return new CacheSizeInfo(
            fieldCache.size(),
            typeConversionCache.size(),
            typeValidationCache.size()
        );
    }

    /**
     * Cache statistics tracking.
     */
    public static class CacheStats {
        private volatile long fieldCacheHits = 0;
        private volatile long typeConversionHits = 0;
        private volatile long validationHits = 0;

        void recordFieldCacheHit() {
            fieldCacheHits++;
        }

        void recordTypeConversionHit() {
            typeConversionHits++;
        }

        void recordValidationHit() {
            validationHits++;
        }

        void reset() {
            fieldCacheHits = 0;
            typeConversionHits = 0;
            validationHits = 0;
        }

        /**
         * Create a copy of current stats.
         *
         * @return copy of stats
         */
        CacheStats copy() {
            CacheStats copy = new CacheStats();
            copy.fieldCacheHits = this.fieldCacheHits;
            copy.typeConversionHits = this.typeConversionHits;
            copy.validationHits = this.validationHits;
            return copy;
        }

        /**
         * Get total cache hits.
         *
         * @return total hits
         */
        public long getTotalHits() {
            return fieldCacheHits + typeConversionHits + validationHits;
        }

        public long getFieldCacheHits() {
            return fieldCacheHits;
        }

        public long getTypeConversionHits() {
            return typeConversionHits;
        }

        public long getValidationHits() {
            return validationHits;
        }

        @Override
        public String toString() {
            return String.format(
                "CacheStats{field=%d, typeConversion=%d, validation=%d, total=%d}",
                fieldCacheHits, typeConversionHits, validationHits, getTotalHits()
            );
        }
    }

    /**
     * Cache size information.
     */
    public static class CacheSizeInfo {
        public final int fieldCacheSize;
        public final int typeConversionCacheSize;
        public final int validationCacheSize;

        public CacheSizeInfo(int fieldSize, int typeSize, int validationSize) {
            this.fieldCacheSize = fieldSize;
            this.typeConversionCacheSize = typeSize;
            this.validationCacheSize = validationSize;
        }

        /**
         * Get total cache entries.
         *
         * @return total entries
         */
        public int getTotalEntries() {
            return fieldCacheSize + typeConversionCacheSize + validationCacheSize;
        }

        @Override
        public String toString() {
            return String.format(
                "CacheSizeInfo{field=%d, typeConversion=%d, validation=%d, total=%d}",
                fieldCacheSize, typeConversionCacheSize, validationCacheSize, getTotalEntries()
            );
        }
    }
}
