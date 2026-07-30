package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;

import java.io.File;
import java.io.IOException;
import java.util.*;

/**
 * Formats the schema registry into the intermediate schema {@code fraiseql compile} reads.
 *
 * <p>Every top-level section is an <b>array</b> of objects and every key is snake_case:
 *
 * <pre>
 * {
 *   "version": "2.0.0",
 *   "types":     [ { "name": "User", "sql_source": "v_user", "fields": [ ... ] } ],
 *   "queries":   [ { "name": "users", "return_type": "User", "returns_list": true } ],
 *   "mutations": [ { "name": "createUser", "return_type": "User", "operation": "insert" } ]
 * }
 * </pre>
 *
 * <p>It used to emit objects keyed by name, with {@code fields} keyed by field name, a
 * camelCase {@code returnType}, arguments as a name&rarr;typeString map, and extra
 * {@code javaClass}/{@code baseType}/{@code isList} keys. The compiler rejected the very
 * first thing it read with {@code invalid type: map, expected a sequence} — no type name,
 * no key name — so <b>no Java-authored schema could be compiled at all</b>, while this
 * class's own Javadoc and README.md:226 both stated that it produced the compiler's
 * format (#851). The array-shaped {@code formatTypesArray} that would have been correct
 * existed already and had no callers.
 *
 * <p>Two representations are decoded here rather than duplicated in the builders:
 * <ul>
 *   <li>{@code returnsArray(true)} is stored as a bracketed return type ({@code "[User]"}),
 *       which becomes {@code return_type: "User"} plus {@code returns_list: true};</li>
 *   <li>argument types are GraphQL type expressions, so a trailing {@code !} means
 *       non-null ({@code "ID!"} &rarr; {@code {"type": "ID", "nullable": false}}).
 *       Without the suffix an argument is optional, matching GraphQL's own default.</li>
 * </ul>
 */
public class SchemaFormatter {
    private static final ObjectMapper mapper = new ObjectMapper();
    private static final String SCHEMA_VERSION = "2.0.0";

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

        root.set("types", formatTypesArray(registry.getAllTypes()));
        root.set("queries", formatQueriesArray(registry.getAllQueries()));
        root.set("mutations", formatMutationsArray(registry.getAllMutations()));

        if (!registry.getAllEnums().isEmpty()) {
            root.set("enums", formatEnumsArray(registry.getAllEnums()));
        }
        if (!registry.getAllInputTypes().isEmpty()) {
            root.set("input_types", formatInputTypesArray(registry.getAllInputTypes()));
        }
        if (!registry.getAllInterfaces().isEmpty()) {
            root.set("interfaces", formatInterfacesArray(registry.getAllInterfaces()));
        }

        return root;
    }

    /**
     * Format minimal types.json (types, enums, input_types, interfaces only).
     * Excludes queries, mutations, subscriptions, observers, and other config.
     * For TOML-based workflow where configuration is separate.
     *
     * @param registry the SchemaRegistry to format
     * @return JSON string representing minimal schema (types only)
     */
    public static String formatMinimalSchema(SchemaRegistry registry) {
        ObjectNode root = mapper.createObjectNode();

        root.set("types", formatTypesArray(registry.getAllTypes()));

        if (!registry.getAllEnums().isEmpty()) {
            root.set("enums", formatEnumsArray(registry.getAllEnums()));
        }
        if (!registry.getAllInputTypes().isEmpty()) {
            root.set("input_types", formatInputTypesArray(registry.getAllInputTypes()));
        }
        if (!registry.getAllInterfaces().isEmpty()) {
            root.set("interfaces", formatInterfacesArray(registry.getAllInterfaces()));
        }

        return root.toString();
    }

    /** Strip a trailing {@code !} and report whether it was there. */
    private static boolean isNonNull(String graphQLType) {
        return graphQLType != null && graphQLType.endsWith("!");
    }

    /** The bare type name, with any GraphQL non-null or list wrapper removed. */
    private static String bareType(String graphQLType) {
        String type = graphQLType == null ? "" : graphQLType.trim();
        if (type.endsWith("!")) {
            type = type.substring(0, type.length() - 1).trim();
        }
        if (type.startsWith("[") && type.endsWith("]")) {
            type = type.substring(1, type.length() - 1).trim();
            if (type.endsWith("!")) {
                type = type.substring(0, type.length() - 1).trim();
            }
        }
        return type;
    }

    /**
     * Drop a single outermost {@code !} from a GraphQL type expression.
     *
     * <p>Nullability travels in the sibling {@code nullable} key, so the type must not
     * also encode it. Output fields survived a {@code !} because the compiler's
     * {@code parse_field_type} strips one; input fields carry the type through verbatim,
     * so {@code "String!"} reached the compiled schema as a type name that is not a type.
     * Emitting the bare name matches every other SDK.
     *
     * <p>Only the outer marker is removed: {@code "[User!]"} keeps its element marker,
     * which is a statement about the elements rather than about the field.
     */
    private static String stripOuterNonNull(String graphQLType) {
        String type = graphQLType == null ? "" : graphQLType.trim();
        return type.endsWith("!") ? type.substring(0, type.length() - 1).trim() : type;
    }

    /** Whether a return type expression denotes a list ({@code "[User]"}). */
    private static boolean isListType(String graphQLType) {
        String type = graphQLType == null ? "" : graphQLType.trim();
        if (type.endsWith("!")) {
            type = type.substring(0, type.length() - 1).trim();
        }
        return type.startsWith("[") && type.endsWith("]");
    }

    /**
     * Emit one field as {@code {"name", "type", "nullable"}} plus optional metadata.
     *
     * <p>{@code baseType} and {@code isList} are deliberately not emitted: the compiler
     * denies unknown fields, and both are already encoded in {@code type}.
     */
    private static ObjectNode formatField(TypeConverter.GraphQLFieldInfo fieldInfo) {
        ObjectNode fieldNode = mapper.createObjectNode();
        fieldNode.put("name", fieldInfo.name);
        fieldNode.put("type", stripOuterNonNull(fieldInfo.getGraphQLType()));
        fieldNode.put("nullable", fieldInfo.nullable);

        if (!fieldInfo.description.isEmpty()) {
            fieldNode.put("description", fieldInfo.description);
        }
        if (fieldInfo.requiresScope != null) {
            fieldNode.put("requires_scope", fieldInfo.requiresScope);
        }
        // #807: `requires_scopes` is a key the compiler does not read, and the compiled
        // schema and runtime field filter represent exactly one required scope. Emitting
        // the array produced a field with no scope at all — silently public. A singleton
        // list is the same requirement as a single scope and is emitted as one; anything
        // longer is refused rather than written as a declaration nothing can honour.
        if (fieldInfo.requiresScopes != null) {
            if (fieldInfo.requiresScopes.length > 1) {
                throw new IllegalStateException(String.format(
                    "Field %s requires %d scopes; multiple required scopes are not "
                        + "supported \u2014 use requiresScope with a single value.",
                    fieldInfo.name, fieldInfo.requiresScopes.length));
            }
            if (fieldInfo.requiresScopes.length == 1) {
                fieldNode.put("requires_scope", fieldInfo.requiresScopes[0]);
            }
        }
        return fieldNode;
    }

    /** Emit {@code arguments} as a list of {@code {name, type, nullable}} objects. */
    private static ArrayNode formatArguments(Map<String, String> arguments) {
        ArrayNode argsArray = mapper.createArrayNode();
        if (arguments == null) {
            return argsArray;
        }
        for (Map.Entry<String, String> arg : arguments.entrySet()) {
            ObjectNode argNode = mapper.createObjectNode();
            argNode.put("name", arg.getKey());
            argNode.put("type", bareType(arg.getValue()));
            argNode.put("nullable", !isNonNull(arg.getValue()));
            argsArray.add(argNode);
        }
        return argsArray;
    }

    /** Emit {@code inject_params} in the nested {@code {source, claim}} form. */
    private static ObjectNode formatInjectParams(Map<String, String> injectParams) {
        ObjectNode ipNode = mapper.createObjectNode();
        for (Map.Entry<String, String> entry : injectParams.entrySet()) {
            String[] parts = entry.getValue().split(":", 2);
            ObjectNode sourceNode = mapper.createObjectNode();
            sourceNode.put("source", parts[0]);
            sourceNode.put("claim", parts.length > 1 ? parts[1] : parts[0]);
            ipNode.set(entry.getKey(), sourceNode);
        }
        return ipNode;
    }

    /**
     * Format all registered types as an array of objects with array-valued fields.
     */
    private static ArrayNode formatTypesArray(Map<String, SchemaRegistry.GraphQLTypeInfo> types) {
        ArrayNode typesArray = mapper.createArrayNode();
        for (SchemaRegistry.GraphQLTypeInfo typeInfo : types.values()) {
            ObjectNode typeNode = mapper.createObjectNode();
            typeNode.put("name", typeInfo.name);
            if (!typeInfo.description.isEmpty()) {
                typeNode.put("description", typeInfo.description);
            }
            if (typeInfo.relay) {
                typeNode.put("relay", true);
            }
            if (typeInfo.isError) {
                typeNode.put("is_error", true);
            }
            if (typeInfo.requiresRole != null) {
                typeNode.put("requires_role", typeInfo.requiresRole);
            }
            if (typeInfo.sqlSource != null) {
                typeNode.put("sql_source", typeInfo.sqlSource);
            }

            ArrayNode fieldsArray = mapper.createArrayNode();
            for (TypeConverter.GraphQLFieldInfo fieldInfo : typeInfo.fields.values()) {
                fieldsArray.add(formatField(fieldInfo));
            }
            typeNode.set("fields", fieldsArray);
            typesArray.add(typeNode);
        }
        return typesArray;
    }

    /** Format all registered queries as an array of objects. */
    private static ArrayNode formatQueriesArray(Map<String, SchemaRegistry.QueryInfo> queries) {
        ArrayNode queriesArray = mapper.createArrayNode();
        for (SchemaRegistry.QueryInfo queryInfo : queries.values()) {
            ObjectNode queryNode = mapper.createObjectNode();
            queryNode.put("name", queryInfo.name);
            queryNode.put("return_type", bareType(queryInfo.returnType));
            queryNode.put("returns_list", isListType(queryInfo.returnType));
            queryNode.put("nullable", queryInfo.nullable);
            queryNode.set("arguments", formatArguments(queryInfo.arguments));

            if (!queryInfo.description.isEmpty()) {
                queryNode.put("description", queryInfo.description);
            }
            if (queryInfo.relay) {
                queryNode.put("relay", true);
            }
            if (queryInfo.sqlSource != null) {
                queryNode.put("sql_source", queryInfo.sqlSource);
            }
            if (queryInfo.cacheTtlSeconds != null) {
                queryNode.put("cache_ttl_seconds", queryInfo.cacheTtlSeconds);
            }
            if (queryInfo.requiresRole != null) {
                queryNode.put("requires_role", queryInfo.requiresRole);
            }
            if (queryInfo.injectParams != null && !queryInfo.injectParams.isEmpty()) {
                queryNode.set("inject_params", formatInjectParams(queryInfo.injectParams));
            }
            if (queryInfo.additionalViews != null && !queryInfo.additionalViews.isEmpty()) {
                ArrayNode viewsArray = mapper.createArrayNode();
                for (String view : queryInfo.additionalViews) {
                    viewsArray.add(view);
                }
                queryNode.set("additional_views", viewsArray);
            }
            if (queryInfo.restPath != null) {
                ObjectNode restNode = mapper.createObjectNode();
                restNode.put("path", queryInfo.restPath);
                restNode.put("method", queryInfo.restMethod);
                queryNode.set("rest", restNode);
            }
            queriesArray.add(queryNode);
        }
        return queriesArray;
    }

    /** Format all registered mutations as an array of objects. */
    private static ArrayNode formatMutationsArray(Map<String, SchemaRegistry.MutationInfo> mutations) {
        ArrayNode mutationsArray = mapper.createArrayNode();
        for (SchemaRegistry.MutationInfo mutationInfo : mutations.values()) {
            ObjectNode mutationNode = mapper.createObjectNode();
            mutationNode.put("name", mutationInfo.name);
            mutationNode.put("return_type", bareType(mutationInfo.returnType));
            mutationNode.put("returns_list", isListType(mutationInfo.returnType));
            mutationNode.put("nullable", mutationInfo.nullable);
            mutationNode.set("arguments", formatArguments(mutationInfo.arguments));

            if (!mutationInfo.description.isEmpty()) {
                mutationNode.put("description", mutationInfo.description);
            }
            if (mutationInfo.sqlSource != null) {
                mutationNode.put("sql_source", mutationInfo.sqlSource);
            }
            if (mutationInfo.operation != null) {
                mutationNode.put("operation", mutationInfo.operation);
            }
            if (mutationInfo.requiresRole != null) {
                mutationNode.put("requires_role", mutationInfo.requiresRole);
            }
            if (mutationInfo.injectParams != null && !mutationInfo.injectParams.isEmpty()) {
                mutationNode.set("inject_params", formatInjectParams(mutationInfo.injectParams));
            }
            if (mutationInfo.invalidatesViews != null && !mutationInfo.invalidatesViews.isEmpty()) {
                ArrayNode viewsArray = mapper.createArrayNode();
                for (String view : mutationInfo.invalidatesViews) {
                    viewsArray.add(view);
                }
                mutationNode.set("invalidates_views", viewsArray);
            }
            if (mutationInfo.invalidatesFactTables != null && !mutationInfo.invalidatesFactTables.isEmpty()) {
                ArrayNode tablesArray = mapper.createArrayNode();
                for (String table : mutationInfo.invalidatesFactTables) {
                    tablesArray.add(table);
                }
                mutationNode.set("invalidates_fact_tables", tablesArray);
            }
            if (mutationInfo.cascade) {
                mutationNode.put("cascade", true);
            }
            if (mutationInfo.restPath != null) {
                ObjectNode restNode = mapper.createObjectNode();
                restNode.put("path", mutationInfo.restPath);
                restNode.put("method", mutationInfo.restMethod);
                mutationNode.set("rest", restNode);
            }
            mutationsArray.add(mutationNode);
        }
        return mutationsArray;
    }

    /** Format all registered enums as an array of objects. */
    private static ArrayNode formatEnumsArray(Map<String, SchemaRegistry.EnumInfo> enums) {
        ArrayNode enumsArray = mapper.createArrayNode();
        for (SchemaRegistry.EnumInfo enumInfo : enums.values()) {
            ObjectNode enumNode = mapper.createObjectNode();
            enumNode.put("name", enumInfo.name);
            if (!enumInfo.description.isEmpty()) {
                enumNode.put("description", enumInfo.description);
            }
            ArrayNode valuesArray = mapper.createArrayNode();
            for (String value : enumInfo.values.keySet()) {
                ObjectNode valueNode = mapper.createObjectNode();
                valueNode.put("name", value);
                valuesArray.add(valueNode);
            }
            enumNode.set("values", valuesArray);
            enumsArray.add(enumNode);
        }
        return enumsArray;
    }

    /** Format all registered input object types as an array of objects. */
    private static ArrayNode formatInputTypesArray(Map<String, SchemaRegistry.InputTypeInfo> inputTypes) {
        ArrayNode inputTypesArray = mapper.createArrayNode();
        for (SchemaRegistry.InputTypeInfo inputInfo : inputTypes.values()) {
            ObjectNode inputNode = mapper.createObjectNode();
            inputNode.put("name", inputInfo.name);
            if (!inputInfo.description.isEmpty()) {
                inputNode.put("description", inputInfo.description);
            }
            ArrayNode fieldsArray = mapper.createArrayNode();
            for (TypeConverter.GraphQLFieldInfo fieldInfo : inputInfo.fields.values()) {
                fieldsArray.add(formatField(fieldInfo));
            }
            inputNode.set("fields", fieldsArray);
            inputTypesArray.add(inputNode);
        }
        return inputTypesArray;
    }

    /** Format all registered interfaces as an array of objects. */
    private static ArrayNode formatInterfacesArray(Map<String, SchemaRegistry.InterfaceInfo> interfaces) {
        ArrayNode interfacesArray = mapper.createArrayNode();
        for (SchemaRegistry.InterfaceInfo interfaceInfo : interfaces.values()) {
            ObjectNode interfaceNode = mapper.createObjectNode();
            interfaceNode.put("name", interfaceInfo.name);
            if (!interfaceInfo.description.isEmpty()) {
                interfaceNode.put("description", interfaceInfo.description);
            }
            ArrayNode fieldsArray = mapper.createArrayNode();
            for (TypeConverter.GraphQLFieldInfo fieldInfo : interfaceInfo.fields.values()) {
                fieldsArray.add(formatField(fieldInfo));
            }
            interfaceNode.set("fields", fieldsArray);
            interfacesArray.add(interfaceNode);
        }
        return interfacesArray;
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
     * Write a JSON string to file (pretty-printed by re-parsing).
     *
     * @param jsonString the JSON string to write
     * @param filePath the output file path
     * @throws IOException if writing to file fails
     */
    public static void writeToFile(String jsonString, String filePath) throws IOException {
        writeToFile(jsonString, filePath, true);
    }

    /**
     * Write a JSON string to file with optional pretty-printing.
     *
     * @param jsonString the JSON string to write
     * @param filePath the output file path
     * @param pretty whether to pretty-print JSON
     * @throws IOException if writing to file fails
     */
    public static void writeToFile(String jsonString, String filePath, boolean pretty) throws IOException {
        File file = new File(filePath);
        if (pretty) {
            Object parsed = mapper.readValue(jsonString, Object.class);
            mapper.writerWithDefaultPrettyPrinter().writeValue(file, parsed);
        } else {
            new java.io.FileWriter(file).append(jsonString).close();
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
