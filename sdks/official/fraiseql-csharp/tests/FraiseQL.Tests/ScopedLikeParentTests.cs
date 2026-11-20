using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// <c>ScopedLikeParent</c> authoring: a field that embeds a type whose own query is scoped
/// by an injected parameter declares it scoped like its parent.
/// </summary>
/// <remarks>
/// The compiler checks the claim; the SDK's part is to carry the declaration, and only
/// where it was made.
/// </remarks>
[Collection(RegistryTestCollection.Name)]
public sealed class ScopedLikeParentTests : IDisposable
{
    public ScopedLikeParentTests() => SchemaRegistry.Instance.Clear();

    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "OrderLine", SqlSource = "v_order_line")]
    private sealed class OrderLineFixture
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "Order", SqlSource = "v_order")]
    private sealed class OrderFixture
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "[OrderLine!]", ScopedLikeParent = true)]
        public List<OrderLineFixture> Lines { get; set; } = new();

        [GraphQLField(Type = "OrderLine")]
        public OrderLineFixture FirstLine { get; set; } = new();
    }

    [GraphQLType(Name = "OrderInput", IsInput = true)]
    private sealed class OrderInputFixture
    {
        [GraphQLField(Type = "ID", ScopedLikeParent = true)]
        public string LineId { get; set; } = string.Empty;
    }

    private static Dictionary<string, JsonElement> Fields(JsonElement root, string typeName)
    {
        foreach (var item in root.GetProperty("types").EnumerateArray())
        {
            if (item.GetProperty("name").GetString() != typeName)
                continue;
            return item.GetProperty("fields").EnumerateArray()
                .ToDictionary(c => c.GetProperty("name").GetString()!, c => c.Clone());
        }
        throw new Xunit.Sdk.XunitException($"type {typeName} is absent from the exported schema");
    }

    private static void AssertOnlyLinesDeclaresIt(JsonElement root)
    {
        var order = Fields(root, "Order");
        Assert.True(order["lines"].GetProperty("scoped_like_parent").GetBoolean(), order["lines"].ToString());
        Assert.False(order["firstLine"].TryGetProperty("scoped_like_parent", out _), "a sibling field carries no key");
        Assert.False(order["id"].TryGetProperty("scoped_like_parent", out _), "a sibling field carries no key");
        var line = Fields(root, "OrderLine");
        Assert.False(line["id"].TryGetProperty("scoped_like_parent", out _), "another type's field carries no key");
    }

    [Fact]
    public void TheKeyIsExportedOnlyOnTheFieldThatDeclaresIt()
    {
        SchemaRegistry.Instance.Register(typeof(OrderLineFixture));
        SchemaRegistry.Instance.Register(typeof(OrderFixture));

        using var document = JsonDocument.Parse(SchemaExporter.Export());
        AssertOnlyLinesDeclaresIt(document.RootElement);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredTypesDeclaration()
    {
        SchemaRegistry.Instance.Register(typeof(OrderLineFixture));
        SchemaRegistry.Instance.Register(typeof(OrderFixture));

        using var document = JsonDocument.Parse(new SchemaBuilder().Export());
        AssertOnlyLinesDeclaresIt(document.RootElement);
    }

    [Fact]
    public void TheSchemaBuilderDeclaresAScopedLikeParentField()
    {
        var json = new SchemaBuilder()
            .Type("OrderLine", t => t
                .SqlSource("v_order_line")
                .Field("id", "ID"))
            .Type("Order", t => t
                .SqlSource("v_order")
                .Field("id", "ID")
                .Field("lines", "[OrderLine!]", scopedLikeParent: true)
                .Field("firstLine", "OrderLine"))
            .Export();

        using var document = JsonDocument.Parse(json);
        AssertOnlyLinesDeclaresIt(document.RootElement);
    }

    [Fact]
    public void AnInputFieldCannotDeclareIt()
    {
        var refused = Assert.Throws<InvalidOperationException>(
            () => SchemaRegistry.Instance.Register(typeof(OrderInputFixture)));
        Assert.Contains("lineId", refused.Message);
        Assert.Contains("ScopedLikeParent", refused.Message);
        Assert.Empty(SchemaRegistry.Instance.GetAllTypes());
    }
}
