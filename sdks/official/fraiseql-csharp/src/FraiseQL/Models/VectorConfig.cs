using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// pgvector configuration for a vector field, emitted as the <c>vector_config</c> object
/// the compiler reads.
/// </summary>
/// <remarks>
/// The compiler refuses a <c>Vector</c>, <c>BitVector</c>, <c>HalfVector</c> or
/// <c>SparseVector</c> field that carries no configuration, so this is what makes those
/// types authorable.
/// <para>
/// Which combinations of field type, metric and index exist is pgvector's business and
/// the compiler's: it holds the operator-class table — <c>ivfflat</c> has no class for a
/// sparse vector at all, and none for jaccard — and refuses a schema that asks for one
/// that does not, naming the alternative. This SDK carries no second copy of that table;
/// a copy is what drifts.
/// </para>
/// </remarks>
/// <param name="Dimensions">
/// Vector width: float components for <c>Vector</c>, <c>HalfVector</c> and
/// <c>SparseVector</c>, <b>bits</b> for <c>BitVector</c>. It sizes the column, and a
/// query vector of a different width is refused rather than silently padded.
/// </param>
/// <param name="IndexType">One of the <see cref="VectorIndex"/> constants.</param>
/// <param name="DistanceMetric">One of the <see cref="VectorMetric"/> constants.</param>
public record VectorConfig(
    [property: JsonPropertyName("dimensions")]      int Dimensions,
    [property: JsonPropertyName("index_type")]      string IndexType = VectorIndex.Hnsw,
    [property: JsonPropertyName("distance_metric")] string DistanceMetric = VectorMetric.Cosine);

/// <summary>The index a pgvector column is searched through.</summary>
public static class VectorIndex
{
    /// <summary>Hierarchical Navigable Small World index — the default.</summary>
    public const string Hnsw = "hnsw";

    /// <summary>Inverted-file index: smaller and faster to build, slower to query.</summary>
    public const string IvfFlat = "ivf_flat";

    /// <summary>No index — exact search.</summary>
    public const string None = "none";
}

/// <summary>The distance metric a vector search orders by.</summary>
public static class VectorMetric
{
    /// <summary>Cosine distance — the default, and what most text embeddings want.</summary>
    public const string Cosine = "cosine";

    /// <summary>Euclidean distance.</summary>
    public const string L2 = "l2";

    /// <summary>Negative inner product.</summary>
    public const string InnerProduct = "inner_product";

    /// <summary>Differing bits — <c>BitVector</c> only.</summary>
    public const string Hamming = "hamming";

    /// <summary>Set overlap normalised by set size — <c>BitVector</c> only.</summary>
    public const string Jaccard = "jaccard";
}
