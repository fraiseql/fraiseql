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

            fields.put(fieldName, new GraphQLFieldInfo(
                fieldName,
                graphQLType,
                nullable,
                annotation.description()
            ));
        }

        return fields;
    }

    /**
     * Information about a GraphQL field.
     */
    public static class GraphQLFieldInfo {
        public final String name;
        public final String type;
        public final boolean nullable;
        public final String description;

        public GraphQLFieldInfo(String name, String type, boolean nullable, String description) {
            this.name = name;
            this.type = type;
            this.nullable = nullable;
            this.description = description;
        }

        @Override
        public String toString() {
            return String.format("{name='%s', type='%s', nullable=%b}", name, type, nullable);
        }
    }
}
