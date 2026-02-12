package com.fraiseql.core;

/**
 * Thrown when custom scalar validation fails.
 *
 * <p>Provides context about which scalar failed, in what context, and the underlying error message.
 */
public class ScalarValidationError extends RuntimeException {

    private final String scalarName;
    private final String context;

    /**
     * Create a new ScalarValidationError.
     *
     * @param scalarName the name of the scalar that failed validation
     * @param context the validation context ("serialize", "parseValue", or "parseLiteral")
     * @param message the underlying error message
     */
    public ScalarValidationError(String scalarName, String context, String message) {
        super(
            String.format(
                "Scalar \"%s\" validation failed in %s: %s",
                scalarName, context, message));
        this.scalarName = scalarName;
        this.context = context;
    }

    /**
     * Create a new ScalarValidationError with a cause.
     *
     * @param scalarName the name of the scalar that failed validation
     * @param context the validation context ("serialize", "parseValue", or "parseLiteral")
     * @param message the underlying error message
     * @param cause the underlying exception
     */
    public ScalarValidationError(String scalarName, String context, String message, Throwable cause) {
        super(
            String.format(
                "Scalar \"%s\" validation failed in %s: %s",
                scalarName, context, message),
            cause);
        this.scalarName = scalarName;
        this.context = context;
    }

    /**
     * Get the name of the scalar that failed validation.
     *
     * @return the scalar name
     */
    public String getScalarName() {
        return scalarName;
    }

    /**
     * Get the validation context where the error occurred.
     *
     * @return the context ("serialize", "parseValue", or "parseLiteral")
     */
    public String getContext() {
        return context;
    }
}
