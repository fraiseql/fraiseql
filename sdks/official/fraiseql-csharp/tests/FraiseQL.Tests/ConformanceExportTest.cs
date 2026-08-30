using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Models;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// Authors the cross-SDK conformance fixture with the C# SDK's public API.
///
/// Driven by <c>sdks/official/conformance/run.py</c>; see
/// <c>sdks/official/conformance/README.md</c>.
///
/// The one rule for every SDK's copy of this file: author through the SDK, never
/// hand-assemble the JSON. The pre-existing <c>GenerateParitySchema</c> built the expected
/// document with <c>JsonArray</c>/<c>JsonObject</c> literals and never called
/// <see cref="SchemaExporter"/>, which is why it stayed green while the exporter dropped
/// every Relay, IsError and IsInput flag the registry had captured (#849).
/// </summary>
[Collection("SchemaRegistry")]
public class ConformanceExportTest
{
    [Fact]
    public void ConformanceExport()
    {
        var fixture = Environment.GetEnvironmentVariable("FRAISEQL_CONFORMANCE_FIXTURE");
        var output = Environment.GetEnvironmentVariable("FRAISEQL_CONFORMANCE_OUT");
        if (fixture is null || output is null)
        {
            // Not being driven by the harness — nothing to do. Failing here would break
            // an ordinary `dotnet test`.
            return;
        }

        SchemaRegistry.Instance.Clear();
        switch (fixture)
        {
            case "minimal":
                AuthorMinimal();
                break;
            case "full":
                AuthorFull();
                break;
            default:
                throw new ArgumentException($"unknown fixture {fixture}");
        }

        SchemaExporter.ExportToFile(output);
    }

    private static void AuthorMinimal()
    {
        SchemaRegistry.Instance.Register(typeof(ConformanceMinimalUser));

        QueryBuilder.Query("users")
            .ReturnType("User")
            .ReturnsList()
            .SqlSource("v_user")
            .Register();
    }

    private static void AuthorFull()
    {
        SchemaRegistry.Instance.Register(typeof(ConformanceUser));
        SchemaRegistry.Instance.Register(typeof(ConformanceOrder));
        SchemaRegistry.Instance.Register(typeof(ConformanceSupportTicket));
        SchemaRegistry.Instance.Register(typeof(ConformanceUserNotFound));
        SchemaRegistry.Instance.Register(typeof(ConformanceDocument));
        SchemaRegistry.Instance.Register(typeof(ConformanceCreateUserInput));

        SchemaRegistry.Instance.RegisterEnum("OrderStatus", new[] { "PENDING", "SHIPPED", "CANCELLED" });

        QueryBuilder.Query("users")
            .ReturnType("User")
            .ReturnsList()
            .SqlSource("v_user")
            .Register();

        QueryBuilder.Query("user")
            .ReturnType("User")
            .Nullable()
            .SqlSource("v_user")
            .Argument("id", "ID")
            .Register();

        QueryBuilder.Query("tenantOrders")
            .ReturnType("Order")
            .ReturnsList()
            .SqlSource("v_order")
            .Inject("tenant_id", "jwt:tenant_id")
            .CacheTtlSeconds(300)
            .RequiresRole("admin")
            .Register();

        MutationBuilder.Mutation("createUser")
            .ReturnType("User")
            .SqlSource("fn_create_user")
            .Operation("insert")
            .Argument("email", "String")
            .Argument("name", "String", nullable: true)
            .InvalidatesViews("v_user", "v_user_summary")
            .InvalidatesFactTables("tf_signup")
            .Register();

        MutationBuilder.Mutation("placeOrder")
            .ReturnType("Order")
            .SqlSource("fn_place_order")
            .Operation("insert")
            .Inject("user_id", "jwt:sub")
            .InvalidatesViews("v_order_summary")
            .InvalidatesFactTables("tf_sale")
            .Register();
    }

    [GraphQLType(Name = "User", SqlSource = "v_user")]
    private sealed class ConformanceMinimalUser
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "String")]
        public string Email { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "User", SqlSource = "v_user", Relay = true)]
    private sealed class ConformanceUser
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "String")]
        public string Email { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Nullable = true, Description = "The user's \"display\" name",
            Deprecated = "use displayName")]
        public string? Name { get; set; }

        [GraphQLField(Type = "Float", Nullable = true, Scope = "read:User.salary")]
        public double? Salary { get; set; }
    }

    [GraphQLType(Name = "Document", SqlSource = "v_document")]
    private sealed class ConformanceDocument
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "Vector",
            VectorDimensions = 1536,
            VectorIndexType = VectorIndex.IvfFlat,
            VectorDistanceMetric = VectorMetric.L2)]
        public double[] Embedding { get; set; } = [];

        [GraphQLField(Type = "BitVector",
            VectorDimensions = 768,
            VectorDistanceMetric = VectorMetric.Hamming)]
        public string Fingerprint { get; set; } = string.Empty;

        [GraphQLField(Type = "HalfVector", Nullable = true,
            VectorDimensions = 1536,
            VectorDistanceMetric = VectorMetric.InnerProduct)]
        public double[]? Compact { get; set; }

        [GraphQLField(Type = "SparseVector", Nullable = true,
            VectorDimensions = 30000,
            VectorIndexType = VectorIndex.None)]
        public string? Terms { get; set; }

        [GraphQLField(Type = "Float", VectorDistance = "embedding")]
        public double Similarity { get; set; }
    }

    [GraphQLType(Name = "Order", SqlSource = "v_order")]
    private sealed class ConformanceOrder
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "Float")]
        public double Total { get; set; }

        [GraphQLField(Type = "String")]
        public string Status { get; set; } = string.Empty;
    }

    // `Crud` is an authoring-time expansion the compiler has no concept of, so the only
    // evidence this SDK implements it is that the operations and input objects appear in
    // the compiled schema. `Computed` is the same: emitting the flag makes the document
    // uncompilable, so the sole evidence it was honoured is `slug` on the type and absent
    // from both input objects.
    [GraphQLType(Name = "SupportTicket", SqlSource = "v_support_ticket", Crud = true)]
    private sealed class ConformanceSupportTicket
    {
        [GraphQLField(Type = "Int")]
        public int Id { get; set; }

        [GraphQLField(Type = "String")]
        public string Title { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Computed = true)]
        public string Slug { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "UserNotFound", SqlSource = "v_user_not_found", IsError = true)]
    private sealed class ConformanceUserNotFound
    {
        [GraphQLField(Type = "String")]
        public string Message { get; set; } = string.Empty;

        [GraphQLField(Type = "String")]
        public string Code { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "CreateUserInput", IsInput = true)]
    private sealed class ConformanceCreateUserInput
    {
        [GraphQLField(Type = "String")]
        public string Email { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Nullable = true)]
        public string? Name { get; set; }
    }
}
