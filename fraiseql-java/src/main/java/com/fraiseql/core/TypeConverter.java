package com.fraiseql.core;

import java.lang.reflect.Field;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.util.*;

/**
 * Converts Java types to GraphQL types.
 * Supports all common Java types, nullable types via Optional, and custom types.
 */
public class TypeConverter {

    /**
     * Converts a Java type to a GraphQL type string.
     *
     * @param javaType the Java type to convert
     * @return the GraphQL type string (e.g., "Int", "String", "Boolean")
     */
    public static String javaToGraphQL(Class<?> javaType) {
        // Handle primitive types
        if (javaType == int.class || javaType == Integer.class) {
            return "Int";
        }
        if (javaType == long.class || javaType == Long.class) {
            return "Int";  // GraphQL doesn't distinguish long vs int
        }
        if (javaType == float.class || javaType == Float.class ||
            javaType == double.class || javaType == Double.class) {
            return "Float";
        }
        if (javaType == boolean.class || javaType == Boolean.class) {
            return "Boolean";
        }
        if (javaType == String.class) {
            return "String";
        }

        // Handle temporal types
        if (javaType == LocalDate.class || javaType == LocalDateTime.class ||
            javaType == java.util.Date.class || javaType == java.sql.Date.class) {
            return "String";  // Date types are represented as strings in GraphQL
        }

        // Handle collections
        if (Collection.class.isAssignableFrom(javaType) || javaType.isArray()) {
            return "[" + javaToGraphQL(Object.class) + "]";
        }

        // Handle Optional
        if (javaType == Optional.class) {
            return "String";  // Default fallback for bare Optional
        }

        // Custom types (classes with @GraphQLType annotation)
        if (javaType.isAnnotationPresent(GraphQLType.class)) {
            GraphQLType annotation = javaType.getAnnotation(GraphQLType.class);
            if (!annotation.name().isEmpty()) {
                return annotation.name();
            }
            return javaType.getSimpleName();
        }

        // Default: use class name
        return javaType.getSimpleName();
    }

    /**
     * Extracts field information from a class annotated with @GraphQLType.
     *
     * @param type the class to analyze
     * @return a map of field names to GraphQL type information
     */
    public static Map<String, GraphQLFieldInfo> extractFields(Class<?> type) {
        Map<String, GraphQLFieldInfo> fields = new LinkedHashMap<>();

        if (!type.isAnnotationPresent(GraphQLType.class)) {
            return fields;
        }

        for (Field field : type.getDeclaredFields()) {
            if (!field.isAnnotationPresent(GraphQLField.class)) {
                continue;
            }

            GraphQLField annotation = field.getAnnotation(GraphQLField.class);
            String fieldName = annotation.name().isEmpty() ? field.getName() : annotation.name();
            String graphQLType = annotation.type().isEmpty() ?
                javaToGraphQL(field.getType()) : annotation.type();
            boolean nullable = annotation.nullable();

            // Extract scope information
            String requiresScope = annotation.requiresScope().isEmpty() ? null : annotation.requiresScope();
            String[] requiresScopes = annotation.requiresScopes().length == 0 ? null : annotation.requiresScopes();

            // Validate scopes if present
            if (requiresScope != null) {
                validateScope(requiresScope, type.getSimpleName(), fieldName);
            }
            if (requiresScopes != null) {
                for (String scope : requiresScopes) {
                    validateScope(scope, type.getSimpleName(), fieldName);
                }
            }

            // Ensure at most one of requiresScope or requiresScopes is set
            if (requiresScope != null && requiresScopes != null) {
                throw new RuntimeException(
                    String.format("Field %s.%s cannot have both requiresScope and requiresScopes",
                        type.getSimpleName(), fieldName)
                );
            }

            GraphQLFieldInfo fieldInfo = new GraphQLFieldInfo(
                fieldName,
                graphQLType,
                nullable,
                annotation.description(),
                requiresScope,
                requiresScopes
            );

            // Set deprecation flag if deprecated reason is provided
            if (!annotation.deprecated().isEmpty()) {
                fieldInfo.isDeprecated = true;
            }

            fields.put(fieldName, fieldInfo);
        }

        return fields;
    }

    /**
     * Validates scope format: action:resource where:
     * - action: alphanumeric + underscore (e.g., read, write, admin)
     * - resource: alphanumeric + underscore + dot + asterisk (e.g., user.email, User.*, *)
     *
     * Valid patterns:
     * - read:user.email
     * - write:User.salary
     * - admin:*
     * - read:*
     * - *
     */
    private static void validateScope(String scope, String typeName, String fieldName) {
        if (scope == null || scope.isEmpty()) {
            throw new RuntimeException(
                String.format("Field %s.%s has empty scope", typeName, fieldName)
            );
        }

        // Global wildcard is always valid
        if ("*".equals(scope)) {
            return;
        }

        // Must contain at least one colon
        if (!scope.contains(":")) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid scope '%s' (missing colon)", typeName, fieldName, scope)
            );
        }

        String[] parts = scope.split(":", 2);
        if (parts.length != 2) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid scope '%s'", typeName, fieldName, scope)
            );
        }

        String action = parts[0];
        String resource = parts[1];

        // Validate action: alphanumeric + underscore
        if (!action.matches("[a-zA-Z_][a-zA-Z0-9_]*")) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid action in scope '%s' (must be alphanumeric + underscore)",
                    typeName, fieldName, scope)
            );
        }

        // Validate resource: alphanumeric + underscore + dot + asterisk
        if (!resource.matches("[a-zA-Z_][a-zA-Z0-9_.]*|\\*")) {
            throw new RuntimeException(
                String.format("Field %s.%s has invalid resource in scope '%s' (must be alphanumeric + underscore + dot, or *)",
                    typeName, fieldName, scope)
            );
        }
    }

    /**
     * Information about a GraphQL field.
     */
    public static class GraphQLFieldInfo {
        public final String name;
        public final String type;
        public final boolean nullable;
        public final String description;
        public final String requiresScope;
        public final String[] requiresScopes;
        public boolean isDeprecated;

        public GraphQLFieldInfo(String name, String type, boolean nullable, String description) {
            this(name, type, nullable, description, null, null);
        }

        public GraphQLFieldInfo(String name, String type, boolean nullable, String description,
                              String requiresScope, String[] requiresScopes) {
            this.name = name;
            this.type = type;
            this.nullable = nullable;
            this.description = description;
            this.requiresScope = requiresScope;
            this.requiresScopes = requiresScopes;
            this.isDeprecated = false;
        }

        public String getRequiresScope() {
            return requiresScope;
        }

        public String[] getRequiresScopes() {
            return requiresScopes;
        }

        @Override
        public String toString() {
            return String.format("{name='%s', type='%s', nullable=%b}", name, type, nullable);
        }
    }
}
