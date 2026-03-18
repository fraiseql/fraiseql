package com.fraiseql.core;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.junit.jupiter.api.Test;

import java.io.File;

/**
 * Generate parity schema for cross-SDK comparison.
 *
 * Produces the canonical parity-schema.json in array format compatible with
 * Python / TypeScript / Go / PHP output and the compare_parity_schemas.py script.
 *
 * Run via Maven:
 *   mvn -q test -Dtest=GenerateParitySchema -DschemaOutputFile=/tmp/parity-java.json
 *
 * If schemaOutputFile is not set, the JSON is printed to stdout.
 */
public class GenerateParitySchema {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    @Test
    void generateAndExport() throws Exception {
        ObjectNode root = MAPPER.createObjectNode();

        // ── Types ──────────────────────────────────────────────────────────
        ArrayNode types = MAPPER.createArrayNode();

        types.add(makeType("User", "v_user", false,
            makeField("id",    "ID",     false),
            makeField("email", "String", false),
            makeField("name",  "String", false)
        ));

        types.add(makeType("Order", "v_order", false,
            makeField("id",    "ID",    false),
            makeField("total", "Float", false)
        ));

        ObjectNode userNotFound = makeType("UserNotFound", "v_user_not_found", false,
            makeField("message", "String", false),
            makeField("code",    "String", false)
        );
        userNotFound.put("is_error", true);
        types.add(userNotFound);

        root.set("types", types);

        // ── Queries ────────────────────────────────────────────────────────
        ArrayNode queries = MAPPER.createArrayNode();

        ObjectNode users = MAPPER.createObjectNode();
        users.put("name", "users");
        users.put("return_type", "User");
        users.put("returns_list", true);
        users.put("nullable", false);
        users.put("sql_source", "v_user");
        users.set("arguments", MAPPER.createArrayNode());
        queries.add(users);

        ObjectNode tenantOrders = MAPPER.createObjectNode();
        tenantOrders.put("name", "tenantOrders");
        tenantOrders.put("return_type", "Order");
        tenantOrders.put("returns_list", true);
        tenantOrders.put("nullable", false);
        tenantOrders.put("sql_source", "v_order");
        tenantOrders.set("inject_params", makeInjectParam("tenant_id", "jwt", "tenant_id"));
        tenantOrders.put("cache_ttl_seconds", 300);
        tenantOrders.put("requires_role", "admin");
        tenantOrders.set("arguments", MAPPER.createArrayNode());
        queries.add(tenantOrders);

        root.set("queries", queries);

        // ── Mutations ──────────────────────────────────────────────────────
        ArrayNode mutations = MAPPER.createArrayNode();

        ObjectNode createUser = MAPPER.createObjectNode();
        createUser.put("name", "createUser");
        createUser.put("return_type", "User");
        createUser.put("sql_source", "fn_create_user");
        createUser.put("operation", "insert");
        ArrayNode createUserArgs = MAPPER.createArrayNode();
        createUserArgs.add(makeArgument("email", "String", false));
        createUserArgs.add(makeArgument("name",  "String", false));
        createUser.set("arguments", createUserArgs);
        mutations.add(createUser);

        ObjectNode placeOrder = MAPPER.createObjectNode();
        placeOrder.put("name", "placeOrder");
        placeOrder.put("return_type", "Order");
        placeOrder.put("sql_source", "fn_place_order");
        placeOrder.put("operation", "insert");
        placeOrder.set("inject_params", makeInjectParam("user_id", "jwt", "sub"));
        ArrayNode invalidViews = MAPPER.createArrayNode();
        invalidViews.add("v_order_summary");
        placeOrder.set("invalidates_views", invalidViews);
        ArrayNode invalidTables = MAPPER.createArrayNode();
        invalidTables.add("tf_sales");
        placeOrder.set("invalidates_fact_tables", invalidTables);
        placeOrder.set("arguments", MAPPER.createArrayNode());
        mutations.add(placeOrder);

        root.set("mutations", mutations);

        // ── Output ─────────────────────────────────────────────────────────
        String json = MAPPER.writerWithDefaultPrettyPrinter().writeValueAsString(root);
        String outputFile = System.getProperty("schemaOutputFile");
        if (outputFile != null && !outputFile.isEmpty()) {
            MAPPER.writerWithDefaultPrettyPrinter().writeValue(new File(outputFile), root);
        } else {
            System.out.println(json);
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    private static ObjectNode makeType(String name, String sqlSource, boolean isError,
                                       ObjectNode... fields) {
        ObjectNode t = MAPPER.createObjectNode();
        t.put("name", name);
        t.put("sql_source", sqlSource);
        if (isError) {
            t.put("is_error", true);
        }
        ArrayNode fa = MAPPER.createArrayNode();
        for (ObjectNode f : fields) {
            fa.add(f);
        }
        t.set("fields", fa);
        return t;
    }

    private static ObjectNode makeField(String name, String type, boolean nullable) {
        ObjectNode f = MAPPER.createObjectNode();
        f.put("name", name);
        f.put("type", type);
        f.put("nullable", nullable);
        return f;
    }

    private static ObjectNode makeArgument(String name, String type, boolean nullable) {
        ObjectNode a = MAPPER.createObjectNode();
        a.put("name", name);
        a.put("type", type);
        a.put("nullable", nullable);
        return a;
    }

    /** Build: {"<param>": "<source>:<claim>"} — matches Python SDK format. */
    private static ObjectNode makeInjectParam(String param, String source, String claim) {
        ObjectNode ip = MAPPER.createObjectNode();
        ip.put(param, source + ":" + claim);
        return ip;
    }
}
