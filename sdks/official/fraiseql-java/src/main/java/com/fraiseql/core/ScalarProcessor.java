package com.fraiseql.core;

/**
 * Processor for registering @Scalar-annotated classes.
 *
 * <p>Provides static utility methods to register custom scalars with the global registry.
 */
public final class ScalarProcessor {

    private ScalarProcessor() {
        // Utility class - prevent instantiation
    }

    /**
     * Register a @Scalar-annotated class with the global registry.
     *
     * <p>Call this method during application initialization to register custom scalars.
     *
     * <p>Example:
     * <pre>{@code
     * ScalarProcessor.register(Email.class);
     * ScalarProcessor.register(Phone.class);
     * }</pre>
     *
     * @param scalarClass a class annotated with @Scalar that extends CustomScalar
     * @throws IllegalArgumentException if class is not annotated with @Scalar
     * @throws IllegalArgumentException if scalar name is already registered
     */
    public static void register(Class<? extends CustomScalar> scalarClass) {
        // Check that class is annotated with @Scalar
        if (!scalarClass.isAnnotationPresent(Scalar.class)) {
            throw new IllegalArgumentException(
                String.format("Class %s must be annotated with @Scalar", scalarClass.getName()));
        }

        try {
            // Instantiate to get the scalar name
            CustomScalar instance = scalarClass.getDeclaredConstructor().newInstance();
            String scalarName = instance.getName();

            // Validate scalar name
            if (scalarName == null || scalarName.isEmpty()) {
                throw new IllegalArgumentException(
                    String.format(
                        "CustomScalar %s must have a non-empty name",
                        scalarClass.getName()));
            }

            // Register with the global registry
            ScalarRegistry.getInstance().registerScalar(scalarName, scalarClass);

        } catch (IllegalArgumentException e) {
            throw e;
        } catch (Exception e) {
            throw new IllegalArgumentException(
                String.format("Failed to register scalar %s: %s", scalarClass.getName(), e.getMessage()),
                e);
        }
    }

    /**
     * Register multiple @Scalar-annotated classes.
     *
     * <p>Example:
     * <pre>{@code
     * ScalarProcessor.registerAll(Email.class, Phone.class, URL.class);
     * }</pre>
     *
     * @param scalarClasses classes annotated with @Scalar
     * @throws IllegalArgumentException if any class cannot be registered
     */
    @SuppressWarnings("unchecked")
    public static void registerAll(Class<? extends CustomScalar>... scalarClasses) {
        for (Class<? extends CustomScalar> scalarClass : scalarClasses) {
            register(scalarClass);
        }
    }

    /**
     * Unregister a custom scalar (useful for testing).
     *
     * <p>This is primarily for testing purposes. In production, you typically don't need to
     * unregister scalars.
     *
     * @param scalarClass the scalar class to unregister
     */
    public static void unregister(Class<? extends CustomScalar> scalarClass) {
        try {
            CustomScalar instance = scalarClass.getDeclaredConstructor().newInstance();
            String scalarName = instance.getName();
            ScalarRegistry.getInstance().getCustomScalars().remove(scalarName);
        } catch (Exception e) {
            // Silently fail for unregister operations
        }
    }

    /**
     * Clear all registered scalars (useful for testing).
     */
    public static void clearAll() {
        ScalarRegistry.getInstance().clear();
    }
}
