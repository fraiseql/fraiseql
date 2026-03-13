package com.fraiseql.core;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

/**
 * Validation engine for custom GraphQL scalars.
 *
 * <p>Provides utilities to validate custom scalar values in different contexts.
 */
public final class ScalarValidator {

    private ScalarValidator() {
        // Utility class - prevent instantiation
    }

    /**
     * Execute validation for a custom scalar.
     *
     * @param scalarClass the CustomScalar subclass to validate with
     * @param value the value to validate
     * @param context one of "serialize", "parseValue", or "parseLiteral"
     * @return the validated/converted value
     * @throws ScalarValidationError if validation fails
     * @throws IllegalArgumentException if context is unknown
     *
     * @example
     *     <pre>{@code
     * import com.fraiseql.core.*;
     *
     * // Parse a variable value from GraphQL
     * Object emailValue = ScalarValidator.validate(Email.class, "user@example.com", "parseValue");
     * // Returns "user@example.com"
     *
     * // Validation fails
     * try {
     *     ScalarValidator.validate(Email.class, "invalid-email", "parseValue");
     * } catch (ScalarValidationError e) {
     *     System.err.println("Validation error: " + e.getMessage());
     *     // Output: "Scalar "Email" validation failed in parseValue: Invalid email"
     * }
     *     }</pre>
     */
    public static Object validate(Class<? extends CustomScalar> scalarClass, Object value, String context) {
        try {
            // Instantiate the scalar
            CustomScalar scalar = scalarClass.getDeclaredConstructor().newInstance();
            String scalarName = scalar.getName();

            // Execute validation based on context
            switch (context) {
                case "serialize":
                    return scalar.serialize(value);
                case "parseValue":
                    return scalar.parseValue(value);
                case "parseLiteral":
                    return scalar.parseLiteral(value);
                default:
                    throw new IllegalArgumentException("Unknown validation context: " + context);
            }
        } catch (ScalarValidationError e) {
            throw e;
        } catch (IllegalArgumentException e) {
            // "Unknown validation context" errors propagate as-is
            if (e.getMessage() != null && e.getMessage().startsWith("Unknown validation context")) {
                throw e;
            }
            // Other IAEs (e.g. from parseValue) are wrapped in ScalarValidationError
            try {
                CustomScalar scalar = scalarClass.getDeclaredConstructor().newInstance();
                throw new ScalarValidationError(scalar.getName(), context, e.getMessage(), e);
            } catch (ScalarValidationError sve) {
                throw sve;
            } catch (Exception instantiationError) {
                throw new ScalarValidationError(scalarClass.getSimpleName(), context, e.getMessage(), e);
            }
        } catch (Exception e) {
            // Extract scalar name for error message
            try {
                CustomScalar scalar = scalarClass.getDeclaredConstructor().newInstance();
                throw new ScalarValidationError(scalar.getName(), context, e.getMessage(), e);
            } catch (ScalarValidationError sve) {
                throw sve;
            } catch (Exception instantiationError) {
                throw new ScalarValidationError(scalarClass.getSimpleName(), context, e.getMessage(), e);
            }
        }
    }

    /**
     * Convenience method that defaults context to "parseValue".
     *
     * @param scalarClass the CustomScalar subclass to validate with
     * @param value the value to validate
     * @return the validated/converted value
     * @throws ScalarValidationError if validation fails
     */
    public static Object validate(Class<? extends CustomScalar> scalarClass, Object value) {
        return validate(scalarClass, value, "parseValue");
    }

    /**
     * Get all registered custom scalars.
     *
     * @return an unmodifiable map of scalar names to CustomScalar classes
     */
    public static Map<String, Class<? extends CustomScalar>> getAllCustomScalars() {
        return Collections.unmodifiableMap(ScalarRegistry.getInstance().getCustomScalars());
    }
}
