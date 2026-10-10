using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Models;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// <c>localized</c> authoring (#1527): a String field, input field or mutation argument
/// stored as a locale map.
/// </summary>
/// <remarks>
/// These tests follow the declaration to the exported JSON, and refuse it on anything but a
/// String at export, as the Python and TypeScript SDKs refuse <c>Localized[int]</c>.
/// </remarks>
[Collection(RegistryTestCollection.Name)]
public sealed class LocalizedTests : IDisposable
{
    public LocalizedTests() => SchemaRegistry.Instance.Clear();

    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "Product", SqlSource = "v_product")]
    private sealed class ProductFixture
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Nullable = true, Localized = true)]
        public string? Name { get; set; }

        [GraphQLField(Type = "String")]
        public string Sku { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "CreateProductInput", IsInput = true)]
    private sealed class CreateProductInputFixture
    {
        [GraphQLField(Type = "String", Localized = true)]
        public string Name { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "Priced", SqlSource = "v_priced")]
    private sealed class PricedFixture
    {
        [GraphQLField(Type = "Float", Localized = true)]
        public double Price { get; set; }
    }

    private static Dictionary<string, JsonElement> Named(JsonElement root, string section, string name, string key)
    {
        foreach (var item in root.GetProperty(section).EnumerateArray())
        {
            if (item.GetProperty("name").GetString() != name)
                continue;
            return item.GetProperty(key).EnumerateArray()
                .ToDictionary(c => c.GetProperty("name").GetString()!, c => c.Clone());
        }
        throw new Xunit.Sdk.XunitException($"{section} {name} is absent from the exported schema");
    }

    private static bool IsLocalized(JsonElement element) =>
        element.TryGetProperty("localized", out var flag) && flag.GetBoolean();

    [Fact]
    public void ALocalizedFieldAndInputFieldAreExportedAsLocalized()
    {
        SchemaRegistry.Instance.Register(typeof(ProductFixture));
        SchemaRegistry.Instance.Register(typeof(CreateProductInputFixture));
        SchemaRegistry.Instance.RegisterInputType("RenameProductInput",
            [new IntermediateInputField("name", "String", false, Localized: true)]);

        using var document = JsonDocument.Parse(SchemaExporter.Export());
        var fields = Named(document.RootElement, "types", "Product", "fields");
        Assert.True(IsLocalized(fields["name"]), fields["name"].ToString());
        Assert.False(fields["sku"].TryGetProperty("localized", out _), "an unlocalized field carries no key");
        var inputs = Named(document.RootElement, "input_types", "CreateProductInput", "fields");
        Assert.True(IsLocalized(inputs["name"]), inputs["name"].ToString());
        var renamed = Named(document.RootElement, "input_types", "RenameProductInput", "fields");
        Assert.True(IsLocalized(renamed["name"]), renamed["name"].ToString());
    }

    [Fact]
    public void ALocalizedMutationArgumentIsExportedAsALocalizedString()
    {
        MutationBuilder.Mutation("createProduct")
            .ReturnType("Product")
            .SqlSource("fn_create_product")
            .Operation("insert")
            .Argument("sku", "String")
            .LocalizedArgument("name", nullable: true)
            .Register();

        using var document = JsonDocument.Parse(SchemaExporter.Export());
        var args = Named(document.RootElement, "mutations", "createProduct", "arguments");
        Assert.Equal("String", args["name"].GetProperty("type").GetString());
        Assert.True(args["name"].GetProperty("nullable").GetBoolean());
        Assert.True(IsLocalized(args["name"]), args["name"].ToString());
        Assert.False(args["sku"].TryGetProperty("localized", out _), "an unlocalized argument carries no key");
    }

    [Fact]
    public void LocalizedIsRefusedOnANonStringAtExport()
    {
        SchemaRegistry.Instance.Register(typeof(PricedFixture));
        var refused = Assert.Throws<InvalidOperationException>(() => SchemaExporter.Export());
        Assert.Contains("price", refused.Message);
        Assert.Contains("only a String can be localized", refused.Message);
    }

    [Fact]
    public void LocalizedIsRefusedOnANonStringInputFieldAndArgument()
    {
        SchemaRegistry.Instance.RegisterInputType("PriceInput",
            [new IntermediateInputField("amount", "Float", false, Localized: true)]);
        Assert.Contains("amount", Assert.Throws<InvalidOperationException>(() => SchemaExporter.Export()).Message);

        SchemaRegistry.Instance.Clear();
        SchemaRegistry.Instance.RegisterMutation(MutationBuilder.Mutation("setPrice")
            .ReturnType("Product")
            .SqlSource("fn_set_price")
            .Operation("update")
            .Build() with
            {
                Arguments = [new IntermediateArgument("amount", "Float", false, Localized: true)],
            });
        Assert.Contains("amount", Assert.Throws<InvalidOperationException>(() => SchemaExporter.Export()).Message);
    }

    [Fact]
    public void TheSchemaBuilderDeclaresALocalizedField()
    {
        var json = new SchemaBuilder()
            .Type("Product", t => t
                .SqlSource("v_product")
                .Field("name", "String", nullable: true, localized: true)
                .Field("sku", "String"))
            .Export();

        using var document = JsonDocument.Parse(json);
        var fields = Named(document.RootElement, "types", "Product", "fields");
        Assert.True(IsLocalized(fields["name"]), fields["name"].ToString());
        Assert.False(fields["sku"].TryGetProperty("localized", out _));
    }
}
