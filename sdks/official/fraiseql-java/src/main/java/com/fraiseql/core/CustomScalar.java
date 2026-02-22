package com.fraiseql.core;

/**
 * Abstract base class for custom GraphQL scalars with validation.
 *
 * <p>Subclasses must define a scalar name and implement the three validation methods
 * (serialize, parseValue, parseLiteral).
 *
 * <p>Use with the @Scalar annotation to register custom scalars with the schema.
 *
 * <p>Example:
 * <pre>{@code
 * @Scalar
 * public class Email extends CustomScalar {
 *     public String getName() {
 *         return "Email";
 *     }
 *
 *     public Object serialize(Object value) {
 *         return String.valueOf(value);
 *     }
 *
 *     public Object parseValue(Object value) {
 *         String str = String.valueOf(value);
 *         if (!str.contains("@")) {
 *             throw new IllegalArgumentException("Invalid email address");
 *         }
 *         return str;
 *     }
 *
 *     public Object parseLiteral(Object ast) {
 *         if (ast instanceof Map) {
 *             Map<String, Object> astMap = (Map<String, Object>) ast;
 *             if (astMap.containsKey("value")) {
 *                 return parseValue(astMap.get("value"));
 *             }
 *         }
 *         throw new IllegalArgumentException("Invalid email literal");
 *     }
 * }
 * }</pre>
 */
public abstract class CustomScalar {

    /**
     * Get the scalar name (e.g., "Email"). Must be unique in schema.
     *
     * @return the scalar name, must not be null or empty
     */
    public abstract String getName();

    /**
     * Convert value to output format (schema → response).
     *
     * <p>Called when serializing a field value in GraphQL response.
     *
     * @param value the internal representation (from database/object)
     * @return the value formatted for GraphQL response (usually String)
     * @throws IllegalArgumentException or IllegalStateException if value cannot be serialized
     *
     * @example
     *     <pre>{@code
     * public Object serialize(Object value) {
     *     return String.valueOf(value);
     * }
     *     }</pre>
     */
    public abstract Object serialize(Object value);

    /**
     * Validate and convert input value (client input → internal).
     *
     * <p>Called when a scalar is passed as a variable in GraphQL query.
     *
     * @param value raw input value from client
     * @return validated/converted value
     * @throws IllegalArgumentException if validation fails
     *
     * @example
     *     <pre>{@code
     * public Object parseValue(Object value) {
     *     String str = String.valueOf(value);
     *     if (!str.contains("@")) {
     *         throw new IllegalArgumentException("Invalid email address");
     *     }
     *     return str;
     * }
     *     }</pre>
     */
    public abstract Object parseValue(Object value);

    /**
     * Parse GraphQL literal (hardcoded value in query).
     *
     * <p>Called when a scalar is hardcoded in the GraphQL query string (not as a variable).
     *
     * @param ast GraphQL AST node representing the literal (Map-like object with "value" key)
     * @return validated/converted value
     * @throws IllegalArgumentException if literal cannot be parsed
     *
     * @example
     *     <pre>{@code
     * public Object parseLiteral(Object ast) {
     *     if (ast instanceof Map) {
     *         Map<String, Object> astMap = (Map<String, Object>) ast;
     *         if (astMap.containsKey("value")) {
     *             return parseValue(astMap.get("value"));
     *         }
     *     }
     *     throw new IllegalArgumentException("Email literal must be string");
     * }
     *     }</pre>
     */
    public abstract Object parseLiteral(Object ast);

    @Override
    public String toString() {
        return String.format("CustomScalar(%s)", getName());
    }
}
