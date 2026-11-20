using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Models;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// A type registered through <see cref="SchemaRegistry"/> exports the same whichever
/// path serializes it: <see cref="SchemaExporter"/> or a <see cref="SchemaBuilder"/>
/// that merges the registry into its own declarations.
/// </summary>
/// <remarks>
/// The two paths each converted a registered type themselves, and the builder's copy
/// carried fewer options, so a declaration reached the compiler through one path and
/// vanished without a diagnostic through the other. One test per option, so a failure
/// names the option that was lost.
/// </remarks>
[Collection(RegistryTestCollection.Name)]
public sealed class RegistryTypeExportParityTests : IDisposable
{
    public RegistryTypeExportParityTests() => SchemaRegistry.Instance.Clear();

    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "Article", SqlSource = "v_article", Description = "An article", Relay = true)]
    [GraphQLRelationship(Name = "comments", TargetType = "Comment",
        Cardinality = "OneToMany", ForeignKey = "fk_article", ReferencedKey = "id")]
    private sealed class ArticleFixture
    {
        [GraphQLField(Type = "ID")]
        public string Id { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Localized = true)]
        public string Title { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Computed = true)]
        public string Slug { get; set; } = string.Empty;

        [GraphQLField(Type = "Vector",
            VectorDimensions = 8,
            VectorIndexType = VectorIndex.IvfFlat,
            VectorDistanceMetric = VectorMetric.L2)]
        public double[] Embedding { get; set; } = [];

        [GraphQLField(Type = "Float", VectorDistance = "embedding")]
        public double Similarity { get; set; }

        [GraphQLField(Type = "String", Nullable = true, Deprecated = "use title")]
        public string? Headline { get; set; }

        [GraphQLField(Type = "String", Nullable = true, Description = "Editor notes", Scope = "read:Article.notes")]
        public string? Notes { get; set; }
    }

    [GraphQLType(Name = "ArticleRefused", SqlSource = "v_article_refused", IsError = true)]
    private sealed class ArticleRefusedFixture
    {
        [GraphQLField(Type = "String")]
        public string Message { get; set; } = string.Empty;
    }

    private static IntermediateType ThroughTheBuilder(Type fixture, string typeName)
    {
        SchemaRegistry.Instance.Register(fixture);
        return new SchemaBuilder().ToSchema().Types.Single(t => t.Name == typeName);
    }

    private static IntermediateField BuilderField(string fieldName) =>
        ThroughTheBuilder(typeof(ArticleFixture), "Article").Fields.Single(f => f.Name == fieldName);

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredFieldsComputed()
    {
        Assert.True(BuilderField("slug").Computed);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredFieldsVector()
    {
        var vector = BuilderField("embedding").Vector;

        Assert.NotNull(vector);
        Assert.Equal(8, vector.Dimensions);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredFieldsVectorDistance()
    {
        Assert.Equal("embedding", BuilderField("similarity").VectorDistance);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredFieldsDeprecated()
    {
        Assert.Equal(new DeprecationInfo("use title"), BuilderField("headline").Deprecated);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredFieldsLocalized()
    {
        Assert.True(BuilderField("title").Localized);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredTypesRelay()
    {
        Assert.True(ThroughTheBuilder(typeof(ArticleFixture), "Article").Relay);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredTypesIsError()
    {
        Assert.True(ThroughTheBuilder(typeof(ArticleRefusedFixture), "ArticleRefused").IsError);
    }

    [Fact]
    public void TheSchemaBuilderCarriesARegisteredTypesRelationships()
    {
        var relationships = ThroughTheBuilder(typeof(ArticleFixture), "Article").Relationships;

        Assert.NotNull(relationships);
        Assert.Equal("comments", Assert.Single(relationships).Name);
    }

    [Fact]
    public void BothPathsExportTheSameFieldsForARegisteredType()
    {
        SchemaRegistry.Instance.Register(typeof(ArticleFixture));

        var exported = SchemaExporter.ToSchema().Types.Single(t => t.Name == "Article").Fields;
        var built = new SchemaBuilder().ToSchema().Types.Single(t => t.Name == "Article").Fields;

        // Record equality reaches the members the wire format omits (`Computed`).
        Assert.Equal(exported.Count, built.Count);
        foreach (var (expected, actual) in exported.Zip(built))
            Assert.Equal(expected, actual);
    }

    [Fact]
    public void BothPathsSerializeARegisteredTypeIdentically()
    {
        SchemaRegistry.Instance.Register(typeof(ArticleFixture));
        SchemaRegistry.Instance.Register(typeof(ArticleRefusedFixture));

        using var exported = JsonDocument.Parse(SchemaExporter.Export());
        using var built = JsonDocument.Parse(new SchemaBuilder().Export());

        Assert.Equal(
            exported.RootElement.GetProperty("types").GetRawText(),
            built.RootElement.GetProperty("types").GetRawText());
    }
}
