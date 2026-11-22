using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using FraiseQL.Attributes;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// A <c>decimal</c> is the engine's exact <c>Decimal</c> and a <c>Guid</c> its <c>UUID</c>
/// (an <c>id</c> of it <c>ID</c>). An enum or a <c>[GraphQLType]</c> class is the type of
/// its name, and a JSON document is <c>Json</c>. Any other type is refused, naming the
/// property: published as <c>String</c>, it hid the value's type from the schema.
/// </summary>
[Collection(RegistryTestCollection.Name)]
public sealed class ExactScalarTests
{
    public enum Tier { Basic, Gold }

    [GraphQLType(Name = "Issuer", SqlSource = "v_issuer")]
    public sealed class IssuerType
    {
        [GraphQLField] public Guid Id { get; set; }
    }

    private sealed class Quote
    {
        [GraphQLField] public decimal Amount { get; set; }
        [GraphQLField] public decimal? FxRate { get; set; }
        [GraphQLField] public List<decimal> Deltas { get; set; } = new();
        [GraphQLField] public Guid IssuerId { get; set; }
        [GraphQLField] public Guid? RequestId { get; set; }
        [GraphQLField] public Tier Tier { get; set; }
        [GraphQLField] public List<Tier> Tiers { get; set; } = new();
        [GraphQLField] public IssuerType Issuer { get; set; } = new();
        [GraphQLField] public JsonElement Payload { get; set; }
        [GraphQLField] public JsonDocument? Document { get; set; }
        [GraphQLField] public JsonNode? Node { get; set; }
    }

    private sealed class Untyped
    {
        [GraphQLField] public object Anything { get; set; } = new();
        [GraphQLField] public Uri Link { get; set; } = new("https://example.com");
        [GraphQLField] public Dictionary<string, int> Counts { get; set; } = new();
        [GraphQLField(Type = "Json")] public object Declared { get; set; } = new();
    }

    private static (string Type, bool Nullable) Detect(Type owner, string name)
    {
        var prop = owner.GetProperty(name)!;
        return TypeMapper.Detect(prop, prop.GetCustomAttribute<GraphQLFieldAttribute>());
    }

    [Theory]
    [InlineData(nameof(Quote.Amount), "Decimal", false)]
    [InlineData(nameof(Quote.FxRate), "Decimal", true)]
    [InlineData(nameof(Quote.Deltas), "[Decimal!]", false)]
    [InlineData(nameof(Quote.IssuerId), "UUID", false)]
    [InlineData(nameof(Quote.RequestId), "UUID", true)]
    [InlineData(nameof(Quote.Tier), "Tier", false)]
    [InlineData(nameof(Quote.Tiers), "[Tier!]", false)]
    [InlineData(nameof(Quote.Issuer), "Issuer", false)]
    [InlineData(nameof(Quote.Payload), "Json", false)]
    [InlineData(nameof(Quote.Document), "Json", true)]
    [InlineData(nameof(Quote.Node), "Json", true)]
    public void ATypeMapsToItsExactScalarOrItsName(string property, string type, bool nullable)
    {
        Assert.Equal((type, nullable), Detect(typeof(Quote), property));
    }

    [Theory]
    [InlineData(nameof(Untyped.Anything), "Object")]
    [InlineData(nameof(Untyped.Link), "Uri")]
    [InlineData(nameof(Untyped.Counts), "Dictionary")]
    public void AnUnknownTypeIsRefusedNamingTheProperty(string property, string typeName)
    {
        var refused = Assert.Throws<NotSupportedException>(() => Detect(typeof(Untyped), property));
        Assert.Contains($"Untyped.{property}", refused.Message);
        Assert.Contains(typeName, refused.Message);
        Assert.Contains("Type =", refused.Message);
    }

    [Fact]
    public void ANamedTypeIsNotInferred()
    {
        Assert.Equal(("Json", false), Detect(typeof(Untyped), nameof(Untyped.Declared)));
    }

    [Fact]
    public void AGuidIdRegistersAsId()
    {
        SchemaRegistry.Instance.Clear();
        SchemaRegistry.Instance.Register(typeof(IssuerType));
        var field = SchemaRegistry.Instance.GetAllTypes().Single().Fields.Single();
        Assert.Equal(("id", "ID"), (field.Name, field.Type));
        SchemaRegistry.Instance.Clear();
    }
}
