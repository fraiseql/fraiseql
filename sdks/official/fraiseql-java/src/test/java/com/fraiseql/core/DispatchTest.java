package com.fraiseql.core;

import static org.junit.jupiter.api.Assertions.*;

import java.util.*;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

public class DispatchTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    @Test
    void testDispatchExplicitMapping() {
        // Register enum
        registry.registerEnum(
            "TimeInterval",
            new LinkedHashMap<String, String>() {{ put("DAY", "day"); put("WEEK", "week"); put("MONTH", "month"); }},
            null
        );

        // Register query with dispatch mapping
        FraiseQL.query("orders")
            .returnType("Order")
            .returnsArray(true)
            .sqlSourceDispatch("timeInterval", new LinkedHashMap<String, String>() {{
                put("DAY", "tf_orders_day");
                put("WEEK", "tf_orders_week");
                put("MONTH", "tf_orders_month");
            }})
            .arg("timeInterval", "TimeInterval")
            .register();

        SchemaRegistry.Schema schema = SchemaRegistry.getSchema();
        assertFalse(schema.getQueries().isEmpty(), "Query should be registered");

        SchemaRegistry.QueryInfo query = schema.getQueries().get(0);
        assertEquals("orders", query.getName());
        assertNotNull(query.getConfig(), "Query config should not be null");
        assertTrue(query.getConfig().containsKey("sql_source_dispatch"),
            "sql_source_dispatch should be in config");
    }

    @Test
    void testDispatchTemplate() {
        // Register enum
        registry.registerEnum(
            "Environment",
            new LinkedHashMap<String, String>() {{ put("STAGING", "staging"); put("PRODUCTION", "production"); }},
            null
        );

        // Register query with dispatch template
        FraiseQL.query("users")
            .returnType("User")
            .returnsArray(true)
            .sqlSourceDispatchTemplate("env", "v_users_{env}")
            .arg("env", "Environment")
            .register();

        SchemaRegistry.Schema schema = SchemaRegistry.getSchema();
        assertFalse(schema.getQueries().isEmpty());

        SchemaRegistry.QueryInfo query = schema.getQueries().get(0);
        assertNotNull(query.getConfig());
        assertTrue(query.getConfig().containsKey("sql_source_dispatch"),
            "sql_source_dispatch should be in config");
    }

    @Test
    void testDispatchWithOtherArguments() {
        // Register enum
        registry.registerEnum(
            "Shard",
            new LinkedHashMap<String, String>() {{ put("S1", "shard1"); put("S2", "shard2"); }},
            null
        );

        // Register query with dispatch and other arguments
        FraiseQL.query("items")
            .returnType("Item")
            .returnsArray(true)
            .sqlSourceDispatch("shard", new LinkedHashMap<String, String>() {{
                put("S1", "t_items_s1");
                put("S2", "t_items_s2");
            }})
            .arg("shard", "Shard")
            .arg("limit", "Int")
            .arg("offset", "Int")
            .register();

        SchemaRegistry.Schema schema = SchemaRegistry.getSchema();
        SchemaRegistry.QueryInfo query = schema.getQueries().get(0);

        // Verify dispatch config
        assertNotNull(query.getConfig());
        assertTrue(query.getConfig().containsKey("sql_source_dispatch"));

        // Verify other arguments are present
        assertEquals(3, query.getArguments().size(),
            "Should have 3 arguments (shard, limit, offset)");
    }

    @Test
    void testDispatchBuilderChaining() {
        // Register enum
        registry.registerEnum(
            "Type",
            new LinkedHashMap<String, String>() {{ put("A", "a"); put("B", "b"); }},
            null
        );

        // Test builder chaining
        FraiseQL.query("typedItems")
            .returnType("Item")
            .returnsArray(true)
            .sqlSourceDispatch("type", new LinkedHashMap<String, String>() {{
                put("A", "t_items_a");
                put("B", "t_items_b");
            }})
            .arg("type", "Type")
            .description("Get items by type")
            .register();

        SchemaRegistry.Schema schema = SchemaRegistry.getSchema();
        SchemaRegistry.QueryInfo query = schema.getQueries().get(0);

        assertEquals("Get items by type", query.getDescription());
        assertNotNull(query.getConfig());
        assertTrue(query.getConfig().containsKey("sql_source_dispatch"),
            "Dispatch config should be preserved after chaining");
    }

    @Test
    void testMultipleDispatchQueries() {
        // Register enums
        registry.registerEnum(
            "Region",
            new LinkedHashMap<String, String>() {{ put("US", "us"); put("EU", "eu"); put("ASIA", "asia"); }},
            null
        );
        registry.registerEnum(
            "Environment",
            new LinkedHashMap<String, String>() {{ put("DEV", "dev"); put("PROD", "prod"); }},
            null
        );

        // Register first query with region dispatch
        FraiseQL.query("data")
            .returnType("Data")
            .returnsArray(true)
            .sqlSourceDispatch("region", new LinkedHashMap<String, String>() {{
                put("US", "t_data_us");
                put("EU", "t_data_eu");
                put("ASIA", "t_data_asia");
            }})
            .arg("region", "Region")
            .register();

        // Register second query with environment dispatch
        FraiseQL.query("config")
            .returnType("Data")
            .returnsArray(true)
            .sqlSourceDispatch("env", new LinkedHashMap<String, String>() {{
                put("DEV", "t_config_dev");
                put("PROD", "t_config_prod");
            }})
            .arg("env", "Environment")
            .register();

        SchemaRegistry.Schema schema = SchemaRegistry.getSchema();
        assertEquals(2, schema.getQueries().size(), "Should have 2 queries");

        // Verify both have dispatch configs
        for (SchemaRegistry.QueryInfo query : schema.getQueries()) {
            assertNotNull(query.getConfig());
            assertTrue(query.getConfig().containsKey("sql_source_dispatch"),
                "Query " + query.getName() + " should have dispatch config");
        }
    }
}
