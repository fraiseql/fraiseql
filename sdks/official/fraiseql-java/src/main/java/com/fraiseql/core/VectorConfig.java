package com.fraiseql.core;

import java.lang.annotation.*;

/**
 * pgvector configuration for a vector field, carried by {@link GraphQLField#vector()}.
 *
 * <p>The compiler refuses a {@code Vector}, {@code BitVector}, {@code HalfVector} or
 * {@code SparseVector} field that carries no configuration, so this is what makes those
 * types authorable.
 *
 * <p>Which combinations of field type, metric and index exist is pgvector's business and
 * the compiler's: it holds the operator-class table — {@code ivfflat} has no class for a
 * sparse vector at all, and none for jaccard — and refuses a schema that asks for one
 * that does not, naming the alternative. This SDK carries no second copy of that table;
 * a copy is what drifts.
 *
 * <p>Usage:
 * <pre>
 * &#64;GraphQLType(sqlSource = "v_document")
 * public class Document {
 *     &#64;GraphQLField(type = Scalars.VECTOR, vector = &#64;VectorConfig(dimensions = 1536))
 *     public float[] embedding;
 *
 *     &#64;GraphQLField(vectorDistance = "embedding")
 *     public float similarity;
 * }
 * </pre>
 */
@Retention(RetentionPolicy.RUNTIME)
@Target({})
public @interface VectorConfig {
    /** Hierarchical Navigable Small World index — the default. */
    String INDEX_HNSW = "hnsw";

    /** Inverted-file index: smaller and faster to build, slower to query. */
    String INDEX_IVF_FLAT = "ivf_flat";

    /** No index — exact search. */
    String INDEX_NONE = "none";

    /** Cosine distance — the default, and what most text embeddings want. */
    String METRIC_COSINE = "cosine";

    /** Euclidean distance. */
    String METRIC_L2 = "l2";

    /** Negative inner product. */
    String METRIC_INNER_PRODUCT = "inner_product";

    /** Differing bits — {@code BitVector} only. */
    String METRIC_HAMMING = "hamming";

    /** Set overlap normalised by set size — {@code BitVector} only. */
    String METRIC_JACCARD = "jaccard";

    /**
     * Vector width: float components for {@code Vector}, {@code HalfVector} and
     * {@code SparseVector}, <b>bits</b> for {@code BitVector}.
     *
     * <p>It sizes the column, and a query vector of a different width is refused rather
     * than silently padded. The zero default is the "no vector configuration here"
     * sentinel — a column with no dimensions is not a thing an author can mean.
     */
    int dimensions() default 0;

    /** One of the {@code INDEX_*} constants. */
    String indexType() default INDEX_HNSW;

    /** One of the {@code METRIC_*} constants. */
    String distanceMetric() default METRIC_COSINE;
}
