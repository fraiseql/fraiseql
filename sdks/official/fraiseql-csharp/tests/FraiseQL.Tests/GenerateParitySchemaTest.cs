using System.Text.Json;
using System.Text.Json.Nodes;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// Generate parity schema for cross-SDK comparison.
///
/// Usage:
///   SCHEMA_OUTPUT_FILE=/tmp/schema_csharp.json dotnet test --filter GenerateParitySchema
/// </summary>
public class GenerateParitySchemaTest
{
    [Fact]
    public void GenerateParitySchema()
    {
        var root = new JsonObject();

        // ── Types ────────────────────────────────────────────────────────────

        var types = new JsonArray
        {
            MakeType("User", "v_user", false,
                MakeField("id", "ID", false),
                MakeField("email", "String", false),
                MakeField("name", "String", false)),
            MakeType("Order", "v_order", false,
                MakeField("id", "ID", false),
                MakeField("total", "Float", false)),
        };

        var userNotFound = MakeType("UserNotFound", "v_user_not_found", false,
            MakeField("message", "String", false),
            MakeField("code", "String", false));
        userNotFound["is_error"] = true;
        types.Add(userNotFound);

        root["types"] = types;

        // ── Queries ──────────────────────────────────────────────────────────

        var queries = new JsonArray();

        var users = new JsonObject
        {
            ["name"] = "users",
            ["return_type"] = "User",
            ["returns_list"] = true,
            ["nullable"] = false,
            ["sql_source"] = "v_user",
            ["arguments"] = new JsonArray(),
        };
        queries.Add(users);

        var tenantOrders = new JsonObject
        {
            ["name"] = "tenantOrders",
            ["return_type"] = "Order",
            ["returns_list"] = true,
            ["nullable"] = false,
            ["sql_source"] = "v_order",
            ["inject_params"] = new JsonObject { ["tenant_id"] = "jwt:tenant_id" },
            ["cache_ttl_seconds"] = 300,
            ["requires_role"] = "admin",
            ["arguments"] = new JsonArray(),
        };
        queries.Add(tenantOrders);

        root["queries"] = queries;

        // ── Mutations ────────────────────────────────────────────────────────

        var mutations = new JsonArray();

        var createUser = new JsonObject
        {
            ["name"] = "createUser",
            ["return_type"] = "User",
            ["sql_source"] = "fn_create_user",
            ["operation"] = "insert",
            ["arguments"] = new JsonArray
            {
                MakeArgument("email", "String", false),
                MakeArgument("name", "String", false),
            },
        };
        mutations.Add(createUser);

        var placeOrder = new JsonObject
        {
            ["name"] = "placeOrder",
            ["return_type"] = "Order",
            ["sql_source"] = "fn_place_order",
            ["operation"] = "insert",
            ["inject_params"] = new JsonObject { ["user_id"] = "jwt:sub" },
            ["invalidates_views"] = new JsonArray { "v_order_summary" },
            ["invalidates_fact_tables"] = new JsonArray { "tf_sales" },
            ["arguments"] = new JsonArray(),
        };
        mutations.Add(placeOrder);

        root["mutations"] = mutations;

        // ── Output ───────────────────────────────────────────────────────────

        var options = new JsonSerializerOptions { WriteIndented = true };
        var json = root.ToJsonString(options);

        var outputFile = Environment.GetEnvironmentVariable("SCHEMA_OUTPUT_FILE");
        if (!string.IsNullOrEmpty(outputFile))
        {
            File.WriteAllText(outputFile, json);
        }
        else
        {
            Console.WriteLine(json);
        }
    }

    private static JsonObject MakeType(string name, string sqlSource, bool isError,
        params JsonObject[] fields)
    {
        var t = new JsonObject
        {
            ["name"] = name,
            ["sql_source"] = sqlSource,
        };
        if (isError)
        {
            t["is_error"] = true;
        }
        var fa = new JsonArray();
        foreach (var f in fields)
        {
            fa.Add(f);
        }
        t["fields"] = fa;
        return t;
    }

    private static JsonObject MakeField(string name, string type, bool nullable)
    {
        return new JsonObject
        {
            ["name"] = name,
            ["type"] = type,
            ["nullable"] = nullable,
        };
    }

    private static JsonObject MakeArgument(string name, string type, bool nullable)
    {
        return new JsonObject
        {
            ["name"] = name,
            ["type"] = type,
            ["nullable"] = nullable,
        };
    }
}
