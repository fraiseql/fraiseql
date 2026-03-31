package com.fraiseql.core;

import java.util.*;
import java.util.regex.*;

/**
 * Generates standard CRUD queries and mutations for a GraphQL type.
 *
 * <p>When {@code crud = true} is set on {@link GraphQLType}, this generator
 * creates the following operations:
 * <ul>
 *   <li>Read: get-by-ID query + list query with auto_params</li>
 *   <li>Create: insert mutation with all fields as arguments</li>
 *   <li>Update: update mutation with PK required, other fields nullable</li>
 *   <li>Delete: delete mutation with PK only</li>
 * </ul>
 */
public final class CrudGenerator {

    private static final Pattern CAMEL_RE = Pattern.compile("(?<!^)([A-Z])");

    private CrudGenerator() {}

    /**
     * Convert PascalCase to snake_case.
     *
     * @param name PascalCase name (e.g. "OrderItem")
     * @return snake_case name (e.g. "order_item")
     */
    static String pascalToSnake(String name) {
        return CAMEL_RE.matcher(name).replaceAll("_$1").toLowerCase();
    }

    /**
     * Apply basic English pluralization rules.
     *
     * @param name the name to pluralize
     * @return the pluralized name
     */
    static String pluralize(String name) {
        if (name.endsWith("s") && !name.endsWith("ss")) return name;
        for (String suffix : new String[]{"ss", "sh", "ch", "x", "z"}) {
            if (name.endsWith(suffix)) return name + "es";
        }
        if (name.length() >= 2 && name.charAt(name.length() - 1) == 'y'
                && "aeiou".indexOf(name.charAt(name.length() - 2)) < 0) {
            return name.substring(0, name.length() - 1) + "ies";
        }
        return name + "s";
    }

    /**
     * Generate CRUD operations and register them with the given registry.
     *
     * @param typeName  GraphQL type name (e.g. "Product")
     * @param fields    ordered map of field name to field info
     * @param sqlSource SQL view name (e.g. "v_product")
     * @param cascade   whether generated mutations use GraphQL Cascade
     * @param registry  the schema registry to register operations with
     * @throws IllegalArgumentException if fields is empty
     */
    public static void generate(String typeName, Map<String, TypeConverter.GraphQLFieldInfo> fields,
                                String sqlSource, boolean cascade, SchemaRegistry registry) {
        if (fields.isEmpty()) {
            throw new IllegalArgumentException(
                "Type '" + typeName + "' has no fields; cannot generate CRUD operations");
        }

        String snake = pascalToSnake(typeName);
        String view = (sqlSource != null && !sqlSource.isEmpty()) ? sqlSource : "v_" + snake;

        // Get ordered field list; first field is the primary key
        List<Map.Entry<String, TypeConverter.GraphQLFieldInfo>> fieldList = new ArrayList<>(fields.entrySet());
        Map.Entry<String, TypeConverter.GraphQLFieldInfo> pkEntry = fieldList.get(0);
        String pkName = pkEntry.getKey();
        String pkType = pkEntry.getValue().type;

        // Get-by-ID query
        Map<String, String> getArgs = new LinkedHashMap<>();
        getArgs.put(pkName, pkType);
        registry.registerQuery(snake, typeName, getArgs,
            "Get " + typeName + " by ID.", false, view, null, null, null);

        // List query (returns array)
        registry.registerQuery(pluralize(snake), "[" + typeName + "]",
            new LinkedHashMap<>(), "List " + typeName + " records.",
            false, view, null, null, null);

        // Create mutation: all fields as arguments
        Map<String, String> createArgs = new LinkedHashMap<>();
        for (Map.Entry<String, TypeConverter.GraphQLFieldInfo> entry : fieldList) {
            createArgs.put(entry.getKey(), entry.getValue().type);
        }
        registry.registerMutation("create_" + snake, typeName, createArgs,
            "Create a new " + typeName + ".", "fn_create_" + snake, "INSERT",
            null, null, null, cascade);

        // Update mutation: PK required, other fields nullable
        Map<String, String> updateArgs = new LinkedHashMap<>();
        updateArgs.put(pkName, pkType);
        for (int i = 1; i < fieldList.size(); i++) {
            Map.Entry<String, TypeConverter.GraphQLFieldInfo> entry = fieldList.get(i);
            updateArgs.put(entry.getKey(), entry.getValue().type);
        }
        registry.registerMutation("update_" + snake, typeName, updateArgs,
            "Update an existing " + typeName + ".", "fn_update_" + snake, "UPDATE",
            null, null, null, cascade);

        // Delete mutation: PK only
        Map<String, String> deleteArgs = new LinkedHashMap<>();
        deleteArgs.put(pkName, pkType);
        registry.registerMutation("delete_" + snake, typeName, deleteArgs,
            "Delete a " + typeName + ".", "fn_delete_" + snake, "DELETE",
            null, null, null, cascade);
    }
}
