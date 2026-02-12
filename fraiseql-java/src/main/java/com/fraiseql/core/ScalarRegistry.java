package com.fraiseql.core;

import java.util.Collections;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Global registry for custom scalars.
 *
 * <p>Maintains a singleton registry of all custom scalars defined via @Scalar annotation.
 */
public final class ScalarRegistry {

    private static final ScalarRegistry INSTANCE = new ScalarRegistry();

    private final Map<String, Class<? extends CustomScalar>> customScalars = new ConcurrentHashMap<>();

    private ScalarRegistry() {
        // Singleton - prevent direct instantiation
    }

    /**
     * Get the singleton instance of ScalarRegistry.
     *
     * @return the singleton instance
     */
    public static ScalarRegistry getInstance() {
        return INSTANCE;
    }

    /**
     * Register a custom scalar.
     *
     * @param name the scalar name (e.g., "Email")
     * @param scalarClass the CustomScalar subclass
     * @throws IllegalArgumentException if scalar name is not unique
     */
    public void registerScalar(String name, Class<? extends CustomScalar> scalarClass) {
        if (customScalars.containsKey(name)) {
            throw new IllegalArgumentException(
                String.format("Scalar \"%s\" is already registered", name));
        }
        customScalars.put(name, scalarClass);
    }

    /**
     * Get all registered custom scalars.
     *
     * @return an unmodifiable map of scalar names to CustomScalar classes
     */
    public Map<String, Class<? extends CustomScalar>> getCustomScalars() {
        return Collections.unmodifiableMap(customScalars);
    }

    /**
     * Check if a scalar is registered.
     *
     * @param name the scalar name
     * @return true if registered, false otherwise
     */
    public boolean hasScalar(String name) {
        return customScalars.containsKey(name);
    }

    /**
     * Get a registered scalar by name.
     *
     * @param name the scalar name
     * @return the CustomScalar class, or null if not registered
     */
    public Class<? extends CustomScalar> getScalar(String name) {
        return customScalars.get(name);
    }

    /**
     * Clear all registered scalars (useful for testing).
     */
    public void clear() {
        customScalars.clear();
    }

    /**
     * Auto-register all classes with @Scalar annotation.
     *
     * <p>This method scans for @Scalar-annotated classes at runtime.
     * In a real implementation, you might use reflection or annotation processing.
     *
     * @throws IllegalArgumentException if a scalar name is already registered
     */
    public void autoRegisterScalars() {
        // This would be implemented using reflection in a real application
        // For now, it's a placeholder for future enhancement
    }
}
