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

                fieldNode.put("type", fieldInfo.getGraphQLType());
                fieldNode.put("nullable", fieldInfo.nullable);
                fieldNode.put("isList", fieldInfo.isList);
                fieldNode.put("baseType", fieldInfo.type);

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
     * Write formatted schema to file as pretty-printed JSON.
     *
     * @param schema the formatted schema ObjectNode
     * @param filePath the output file path
     * @throws IOException if writing to file fails
     */
    public static void writeToFile(ObjectNode schema, String filePath) throws IOException {
        File file = new File(filePath);
        mapper.writerWithDefaultPrettyPrinter().writeValue(file, schema);
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
