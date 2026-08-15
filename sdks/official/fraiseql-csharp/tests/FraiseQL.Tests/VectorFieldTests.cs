using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Export;
using FraiseQL.Models;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// pgvector field authoring: <c>vector_config</c> and <c>vector_distance</c> (#959).
/// </summary>
/// <remarks>
/// The compiler refuses a <c>Vector</c> field carrying no <c>vector_config</c>, so an SDK
/// that cannot author the config cannot author the type at all. These tests follow the
/// declaration all the way to the exported JSON, because this SDK has already dropped
/// field- and type-level declarations between the registry and the compiler (#849, #807):
/// the registry captured them and the exporter did not carry them across.
/// </remarks>
[Collection(RegistryTestCollection.Name)]
public sealed class VectorFieldTests : IDisposable
{
    public VectorFieldTests() => SchemaRegistry.Instance.Clear();

    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "Document", SqlSource = "v_document")]
    private sealed class DocumentFixture
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

        [GraphQLField(Type = "SparseVector", Nullable = true,
            VectorDimensions = 30000,
            VectorIndexType = VectorIndex.None)]
        public string? Terms { get; set; }

        [GraphQLField(Type = "Vector", VectorDimensions = 8)]
        public double[] Plain { get; set; } = [];

        [GraphQLField(Type = "Float", VectorDistance = "embedding")]
        public double Similarity { get; set; }
    }

    [GraphQLType(Name = "BothVectorAndDistance", SqlSource = "v_document")]
    private sealed class BothVectorAndDistanceFixture
    {
        [GraphQLField(Type = "Vector", VectorDimensions = 8, VectorDistance = "embedding")]
        public double[] Embedding { get; set; } = [];
    }

    [GraphQLType(Name = "NoDimensions", SqlSource = "v_document")]
    private sealed class NoDimensionsFixture
    {
        [GraphQLField(Type = "Vector", VectorDistanceMetric = VectorMetric.L2)]
        public double[] Embedding { get; set; } = [];
    }

    private static JsonElement Field(string typeName, string fieldName)
    {
        using var document = JsonDocument.Parse(SchemaExporter.Export());
        foreach (var type in document.RootElement.GetProperty("types").EnumerateArray())
        {
            if (type.GetProperty("name").GetString() != typeName)
                continue;
            foreach (var field in type.GetProperty("fields").EnumerateArray())
            {
                if (field.GetProperty("name").GetString() == fieldName)
                    return field.Clone();
            }
        }

        throw new Xunit.Sdk.XunitException($"field {typeName}.{fieldName} is absent from the export");
    }

    [Fact]
    public void EveryVectorFieldTypeCarriesItsConfig()
    {
        SchemaRegistry.Instance.Register(typeof(DocumentFixture));

        // Every key is asserted, not just the object's presence: index_type and
        // distance_metric both have compiler-side defaults, so a config that lost them
        // would still compile — to hnsw + cosine, chosen by nobody.
        var embedding = Field("Document", "embedding").GetProperty("vector_config");
        Assert.Equal(1536, embedding.GetProperty("dimensions").GetInt32());
        Assert.Equal("ivf_flat", embedding.GetProperty("index_type").GetString());
        Assert.Equal("l2", embedding.GetProperty("distance_metric").GetString());

        Assert.Equal("hamming", Field("Document", "fingerprint")
            .GetProperty("vector_config").GetProperty("distance_metric").GetString());
        Assert.Equal("none", Field("Document", "terms")
            .GetProperty("vector_config").GetProperty("index_type").GetString());
    }

    [Fact]
    public void TheIndexAndMetricLeftToTheDefaultAreWrittenOut()
    {
        SchemaRegistry.Instance.Register(typeof(DocumentFixture));

        var plain = Field("Document", "plain").GetProperty("vector_config");
        Assert.Equal(8, plain.GetProperty("dimensions").GetInt32());
        Assert.Equal("hnsw", plain.GetProperty("index_type").GetString());
        Assert.Equal("cosine", plain.GetProperty("distance_metric").GetString());
    }

    [Fact]
    public void ADistanceFieldNamesTheVectorItMeasures()
    {
        SchemaRegistry.Instance.Register(typeof(DocumentFixture));

        Assert.Equal("embedding",
            Field("Document", "similarity").GetProperty("vector_distance").GetString());
    }

    [Fact]
    public void AnOrdinaryFieldCarriesNoVectorKeys()
    {
        SchemaRegistry.Instance.Register(typeof(DocumentFixture));

        var id = Field("Document", "id");
        Assert.False(id.TryGetProperty("vector_config", out _));
        Assert.False(id.TryGetProperty("vector_distance", out _));
    }

    [Fact]
    public void AFieldIsAnEmbeddingOrADistanceNotBoth()
    {
        var thrown = Assert.Throws<InvalidOperationException>(
            () => SchemaRegistry.Instance.Register(typeof(BothVectorAndDistanceFixture)));
        Assert.Contains("not both", thrown.Message);
    }

    [Fact]
    public void ADimensionCountNoColumnCanHaveIsRefused()
    {
        var thrown = Assert.Throws<InvalidOperationException>(
            () => SchemaRegistry.Instance.Register(typeof(NoDimensionsFixture)));
        Assert.Contains("at least 1", thrown.Message);
    }
}
