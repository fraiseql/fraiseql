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
 *   <li>Create: insert mutation with a {@code Create{TypeName}Input} input object</li>
 *   <li>Update: update mutation with an {@code Update{TypeName}Input} input object (PK required, others nullable)</li>
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
     * Mark a GraphQL type expression non-null, idempotently.
     *
     * <p>Required-ness of an argument is carried by the type string here — see
     * {@code SchemaFormatter}, which emits {@code {"type": "ID", "nullable": false}} for
     * {@code "ID!"}. Every generated argument omitted it, so a Java-authored
     * {@code createX(input: CreateXInput)} accepted null where the same declaration in
     * every other SDK produced {@code CreateXInput!}.
     *
     * @param type a GraphQL type expression
     * @return the same expression with a single trailing {@code !}
     */
    static String nonNull(String type) {
        return type.endsWith("!") ? type : type + "!";
    }

    /**
     * Convert snake_case to camelCase. Idempotent.
     *
     * <p>The generated operations carried the snake_case name verbatim, so a
     * {@code crud = true} type produced {@code create_support_ticket} in a schema whose
     * hand-authored mutations beside it were {@code createUser} — one SDK emitting two
     * naming conventions, and a different GraphQL API from the one Python generates for
     * the same declaration (#1247). The compiler does not rename: {@code naming_convention}
     * in the document is metadata, so the SDK has to emit the final name.
     *
     * @param name snake_case name (e.g. "create_order_item")
     * @return camelCase name (e.g. "createOrderItem")
     */
    static String snakeToCamel(String name) {
        StringBuilder out = new StringBuilder(name.length());
        boolean upper = false;
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            if (c == '_') {
                upper = true;
            } else {
                out.append(upper ? Character.toUpperCase(c) : c);
                upper = false;
            }
        }
        return out.toString();
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

        // Get-by-ID query.
        //
        // The `!` and the `setQueryMetadata` call are both load-bearing and were both
        // missing (#1247). Argument required-ness travels in the type expression — the
        // formatter reads a trailing `!` — so `pkType` alone declared a lookup whose id
        // may be omitted. And `QueryInfo.nullable` defaults to false, so a get-by-id
        // that obviously can miss was declared non-null, where every other SDK declares
        // it nullable.
        Map<String, String> getArgs = new LinkedHashMap<>();
        getArgs.put(pkName, nonNull(pkType));
        String getName = snakeToCamel(snake);
        registry.registerQuery(getName, typeName, getArgs,
            "Get " + typeName + " by ID.", false, view, null, null, null);
        registry.setQueryMetadata(getName, true, null);

        // List query (returns array)
        registry.registerQuery(snakeToCamel(pluralize(snake)), "[" + typeName + "]",
            new LinkedHashMap<>(), "List " + typeName + " records.",
            false, view, null, null, null);

        // Create mutation: register input type, then use single "input" argument
        String createInputName = "Create" + typeName + "Input";
        Map<String, TypeConverter.GraphQLFieldInfo> createInputFields = new LinkedHashMap<>();
        for (Map.Entry<String, TypeConverter.GraphQLFieldInfo> entry : fieldList) {
            if (!entry.getValue().computed) {
                createInputFields.put(entry.getKey(), entry.getValue());
            }
        }
        registry.registerInputType(createInputName, createInputFields,
            "Input for creating a new " + typeName + ".");

        Map<String, String> createArgs = new LinkedHashMap<>();
        createArgs.put("input", nonNull(createInputName));
        registry.registerMutation(snakeToCamel("create_" + snake), typeName, createArgs,
            "Create a new " + typeName + ".", "fn_create_" + snake, "INSERT",
            null, null, null, cascade);

        // Update mutation: register input type (PK required, others nullable), then use single "input" argument
        String updateInputName = "Update" + typeName + "Input";
        Map<String, TypeConverter.GraphQLFieldInfo> updateInputFields = new LinkedHashMap<>();
        updateInputFields.put(pkName, pkEntry.getValue());
        for (int i = 1; i < fieldList.size(); i++) {
            Map.Entry<String, TypeConverter.GraphQLFieldInfo> entry = fieldList.get(i);
            if (!entry.getValue().computed) {
                TypeConverter.GraphQLFieldInfo original = entry.getValue();
                TypeConverter.GraphQLFieldInfo nullableField = new TypeConverter.GraphQLFieldInfo(
                    original.name, original.type, true, original.description, original.requiresScope, original.requiresScopes, original.computed);
                updateInputFields.put(entry.getKey(), nullableField);
            }
        }
        registry.registerInputType(updateInputName, updateInputFields,
            "Input for updating an existing " + typeName + ".");

        Map<String, String> updateArgs = new LinkedHashMap<>();
        updateArgs.put("input", nonNull(updateInputName));
        registry.registerMutation(snakeToCamel("update_" + snake), typeName, updateArgs,
            "Update an existing " + typeName + ".", "fn_update_" + snake, "UPDATE",
            null, null, null, cascade);

        // Delete mutation: PK only
        Map<String, String> deleteArgs = new LinkedHashMap<>();
        deleteArgs.put(pkName, nonNull(pkType));
        registry.registerMutation(snakeToCamel("delete_" + snake), typeName, deleteArgs,
            "Delete a " + typeName + ".", "fn_delete_" + snake, "DELETE",
            null, null, null, cascade);
    }
}
