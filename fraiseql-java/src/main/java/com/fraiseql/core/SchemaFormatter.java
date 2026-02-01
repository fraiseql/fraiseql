package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;

import java.io.File;
import java.io.IOException;
import java.util.*;

/**
 * Formats the schema registry into JSON structure compatible with FraiseQL compiler.
 * Generates schema.json that is consumed by fraiseql-cli compile.
 *
 * JSON Structure:
 * {
 *   "version": "1.0",
 *   "types": { ... },
 *   "queries": { ... },
 *   "mutations": { ... }
 * }
 */
public class SchemaFormatter {
    private static final ObjectMapper mapper = new ObjectMapper();
    private static final String SCHEMA_VERSION = "1.0";

    private SchemaFormatter() {
        // Utility class
    }

    /**
     * Format the entire schema registry to JSON structure.
     *
     * @param registry the SchemaRegistry to format
     * @return ObjectNode representing the complete schema
     */
    public static ObjectNode formatSchema(SchemaRegistry registry) {
        ObjectNode root = mapper.createObjectNode();

        // Add schema version for compatibility tracking
        root.put("version", SCHEMA_VERSION);

        // Format types
        root.set("types", formatTypes(registry.getAllTypes()));

        // Format queries
        root.set("queries", formatQueries(registry.getAllQueries()));

        // Format mutations
        root.set("mutations", formatMutations(registry.getAllMutations()));

        // Format observers
        if (!registry.getAllObservers().isEmpty()) {
            root.set("observers", formatObservers(registry.getAllObservers()));
        }

        return root;
    }

    /**
     * Format minimal types.json (types, enums, input_types, interfaces only).
     * Excludes queries, mutations, subscriptions, observers, and other config.
     * For TOML-based workflow where configuration is separate.
     *
     * @param registry the SchemaRegistry to format
     * @return ObjectNode representing minimal schema (types only)
     */
    public static ObjectNode formatMinimalSchema(SchemaRegistry registry) {
        ObjectNode root = mapper.createObjectNode();

        // Format types
        root.set("types", formatTypes(registry.getAllTypes()));

        // Format enums
        if (!registry.getAllEnums().isEmpty()) {
            root.set("enums", formatEnums(registry.getAllEnums()));
        }

        // Format input types
        if (!registry.getAllInputTypes().isEmpty()) {
            root.set("input_types", formatInputTypes(registry.getAllInputTypes()));
        }

        // Format interfaces
        if (!registry.getAllInterfaces().isEmpty()) {
            root.set("interfaces", formatInterfaces(registry.getAllInterfaces()));
        }

        return root;
    }

    /**
     * Format all registered types.
     *
     * @param types map of type name to GraphQLTypeInfo
     * @return ObjectNode with formatted types
     */
    private static ObjectNode formatTypes(Map<String, SchemaRegistry.GraphQLTypeInfo> types) {
        ObjectNode typesNode = mapper.createObjectNode();

        for (SchemaRegistry.GraphQLTypeInfo typeInfo : types.values()) {
            ObjectNode typeNode = mapper.createObjectNode();

            // Add type metadata
            typeNode.put("name", typeInfo.name);
            typeNode.put("javaClass", typeInfo.javaClass.getName());

            // Add description if present
            if (!typeInfo.description.isEmpty()) {
                typeNode.put("description", typeInfo.description);
            }

            // Format fields
            ObjectNode fieldsNode = mapper.createObjectNode();
            for (TypeConverter.GraphQLFieldInfo fieldInfo : typeInfo.fields.values()) {
                ObjectNode fieldNode = mapper.createObjectNode();

                fieldNode.put("type", fieldInfo.type);
                fieldNode.put("nullable", fieldInfo.nullable);

                if (!fieldInfo.description.isEmpty()) {
                    fieldNode.put("description", fieldInfo.description);
                }

                fieldsNode.set(fieldInfo.name, fieldNode);
            }

            typeNode.set("fields", fieldsNode);
            typesNode.set(typeInfo.name, typeNode);
        }

        return typesNode;
    }

    /**
     * Format all registered queries.
     *
     * @param queries map of query name to QueryInfo
     * @return ObjectNode with formatted queries
     */
    private static ObjectNode formatQueries(Map<String, SchemaRegistry.QueryInfo> queries) {
        ObjectNode queriesNode = mapper.createObjectNode();

        for (SchemaRegistry.QueryInfo queryInfo : queries.values()) {
            ObjectNode queryNode = mapper.createObjectNode();

            queryNode.put("name", queryInfo.name);
            queryNode.put("returnType", queryInfo.returnType);

            // Format arguments
            ObjectNode argsNode = mapper.createObjectNode();
            for (Map.Entry<String, String> arg : queryInfo.arguments.entrySet()) {
                argsNode.put(arg.getKey(), arg.getValue());
            }
            queryNode.set("arguments", argsNode);

            if (!queryInfo.description.isEmpty()) {
                queryNode.put("description", queryInfo.description);
            }

            queriesNode.set(queryInfo.name, queryNode);
        }

        return queriesNode;
    }

    /**
     * Format all registered mutations.
     *
     * @param mutations map of mutation name to MutationInfo
     * @return ObjectNode with formatted mutations
     */
    private static ObjectNode formatMutations(Map<String, SchemaRegistry.MutationInfo> mutations) {
        ObjectNode mutationsNode = mapper.createObjectNode();

        for (SchemaRegistry.MutationInfo mutationInfo : mutations.values()) {
            ObjectNode mutationNode = mapper.createObjectNode();

            mutationNode.put("name", mutationInfo.name);
            mutationNode.put("returnType", mutationInfo.returnType);

            // Format arguments
            ObjectNode argsNode = mapper.createObjectNode();
            for (Map.Entry<String, String> arg : mutationInfo.arguments.entrySet()) {
                argsNode.put(arg.getKey(), arg.getValue());
            }
            mutationNode.set("arguments", argsNode);

            if (!mutationInfo.description.isEmpty()) {
                mutationNode.put("description", mutationInfo.description);
            }

            mutationsNode.set(mutationInfo.name, mutationNode);
        }

        return mutationsNode;
    }

    /**
     * Format all registered observers.
     *
     * @param observers map of observer name to ObserverInfo
     * @return ArrayNode with formatted observers
     */
    private static ArrayNode formatObservers(Map<String, SchemaRegistry.ObserverInfo> observers) {
        ArrayNode observersArray = mapper.createArrayNode();

        for (SchemaRegistry.ObserverInfo observerInfo : observers.values()) {
            ObjectNode observerNode = mapper.createObjectNode();

            observerNode.put("name", observerInfo.name);
            observerNode.put("entity", observerInfo.entity);
            observerNode.put("event", observerInfo.event);

            if (observerInfo.condition != null && !observerInfo.condition.isEmpty()) {
                observerNode.put("condition", observerInfo.condition);
            }

            // Format actions
            ArrayNode actionsArray = mapper.createArrayNode();
            for (Map<String, Object> action : observerInfo.actions) {
                ObjectNode actionNode = mapper.valueToTree(action);
                actionsArray.add(actionNode);
            }
            observerNode.set("actions", actionsArray);

            // Format retry config
            ObjectNode retryNode = mapper.createObjectNode();
            Map<String, Object> retryMap = observerInfo.retry.toMap();
            for (Map.Entry<String, Object> entry : retryMap.entrySet()) {
                retryNode.putPOJO(entry.getKey(), entry.getValue());
            }
            observerNode.set("retry", retryNode);

            observersArray.add(observerNode);
        }

        return observersArray;
    }

    /**
     * Format all registered enums.
     *
     * @param enums map of enum name to EnumInfo
     * @return ObjectNode with formatted enums
     */
    private static ObjectNode formatEnums(Map<String, SchemaRegistry.EnumInfo> enums) {
        ObjectNode enumsNode = mapper.createObjectNode();

        for (SchemaRegistry.EnumInfo enumInfo : enums.values()) {
            ObjectNode enumNode = mapper.createObjectNode();

            enumNode.put("name", enumInfo.name);

            if (!enumInfo.description.isEmpty()) {
                enumNode.put("description", enumInfo.description);
            }

            // Format enum values
            ArrayNode valuesArray = mapper.createArrayNode();
            for (String value : enumInfo.values.keySet()) {
                ObjectNode valueNode = mapper.createObjectNode();
                valueNode.put("name", value);
                valuesArray.add(valueNode);
            }
            enumNode.set("values", valuesArray);

            enumsNode.set(enumInfo.name, enumNode);
        }

        return enumsNode;
    }

    /**
     * Format all registered input types.
     *
     * @param inputTypes map of input type name to InputTypeInfo
     * @return ObjectNode with formatted input types
     */
    private static ObjectNode formatInputTypes(Map<String, SchemaRegistry.InputTypeInfo> inputTypes) {
        ObjectNode inputTypesNode = mapper.createObjectNode();

        for (SchemaRegistry.InputTypeInfo inputInfo : inputTypes.values()) {
            ObjectNode inputNode = mapper.createObjectNode();

            inputNode.put("name", inputInfo.name);

            if (!inputInfo.description.isEmpty()) {
                inputNode.put("description", inputInfo.description);
            }

            // Format fields
            ObjectNode fieldsNode = mapper.createObjectNode();
            for (TypeConverter.GraphQLFieldInfo fieldInfo : inputInfo.fields.values()) {
                ObjectNode fieldNode = mapper.createObjectNode();

                fieldNode.put("type", fieldInfo.type);
                fieldNode.put("nullable", fieldInfo.nullable);

                if (!fieldInfo.description.isEmpty()) {
                    fieldNode.put("description", fieldInfo.description);
                }

                fieldsNode.set(fieldInfo.name, fieldNode);
            }

            inputNode.set("fields", fieldsNode);
            inputTypesNode.set(inputInfo.name, inputNode);
        }

        return inputTypesNode;
    }

    /**
     * Format all registered interfaces.
     *
     * @param interfaces map of interface name to InterfaceInfo
     * @return ObjectNode with formatted interfaces
     */
    private static ObjectNode formatInterfaces(Map<String, SchemaRegistry.InterfaceInfo> interfaces) {
        ObjectNode interfacesNode = mapper.createObjectNode();

        for (SchemaRegistry.InterfaceInfo interfaceInfo : interfaces.values()) {
            ObjectNode interfaceNode = mapper.createObjectNode();

            interfaceNode.put("name", interfaceInfo.name);

            if (!interfaceInfo.description.isEmpty()) {
                interfaceNode.put("description", interfaceInfo.description);
            }

            // Format fields
            ObjectNode fieldsNode = mapper.createObjectNode();
            for (TypeConverter.GraphQLFieldInfo fieldInfo : interfaceInfo.fields.values()) {
                ObjectNode fieldNode = mapper.createObjectNode();

                fieldNode.put("type", fieldInfo.type);
                fieldNode.put("nullable", fieldInfo.nullable);

                if (!fieldInfo.description.isEmpty()) {
                    fieldNode.put("description", fieldInfo.description);
                }

                fieldsNode.set(fieldInfo.name, fieldNode);
            }

            interfaceNode.set("fields", fieldsNode);
            interfacesNode.set(interfaceInfo.name, interfaceNode);
        }

        return interfacesNode;
    }

    /**
     * Write formatted schema to file as pretty-printed JSON.
     *
     * @param schema the formatted schema ObjectNode
     * @param filePath the output file path
     * @throws IOException if writing to file fails
     */
    public static void writeToFile(ObjectNode schema, String filePath) throws IOException {
        writeToFile(schema, filePath, true);
    }

    /**
     * Write formatted schema to file with optional pretty-printing.
     *
     * @param schema the formatted schema ObjectNode
     * @param filePath the output file path
     * @param pretty whether to pretty-print JSON
     * @throws IOException if writing to file fails
     */
    public static void writeToFile(ObjectNode schema, String filePath, boolean pretty) throws IOException {
        File file = new File(filePath);
        if (pretty) {
            mapper.writerWithDefaultPrettyPrinter().writeValue(file, schema);
        } else {
            mapper.writeValue(file, schema);
        }
    }

    /**
     * Convert schema to JSON string.
     *
     * @param schema the formatted schema ObjectNode
     * @return JSON string representation
     * @throws IOException if serialization fails
     */
    public static String toJsonString(ObjectNode schema) throws IOException {
        return mapper.writerWithDefaultPrettyPrinter().writeValueAsString(schema);
    }
}
