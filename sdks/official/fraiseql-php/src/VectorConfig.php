<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * pgvector configuration for a vector field.
 *
 * The compiler refuses a `Vector`, `BitVector`, `HalfVector` or `SparseVector` field
 * that carries no configuration, so this is what makes those types authorable.
 *
 * Which combinations of field type, metric and index exist is pgvector's business and
 * the compiler's: it holds the operator-class table — `ivfflat` has no class for a
 * sparse vector at all, and none for jaccard — and refuses a schema that asks for one
 * that does not, naming the alternative. This SDK carries no second copy of that table;
 * a copy is what drifts.
 *
 * Usage:
 * ```php
 * #[GraphQLType(name: 'Document', sqlSource: 'v_document')]
 * final class Document {
 *     #[GraphQLField(type: 'Vector', vectorConfig: new VectorConfig(1536))]
 *     public array $embedding;
 *
 *     #[GraphQLField(type: 'Float', vectorDistance: 'embedding')]
 *     public float $similarity;
 * }
 * ```
 */
final readonly class VectorConfig
{
    /** Hierarchical Navigable Small World index — the default. */
    public const INDEX_HNSW = 'hnsw';

    /** Inverted-file index: smaller and faster to build, slower to query. */
    public const INDEX_IVF_FLAT = 'ivf_flat';

    /** No index — exact search. */
    public const INDEX_NONE = 'none';

    /** Cosine distance — the default, and what most text embeddings want. */
    public const METRIC_COSINE = 'cosine';

    /** Euclidean distance. */
    public const METRIC_L2 = 'l2';

    /** Negative inner product. */
    public const METRIC_INNER_PRODUCT = 'inner_product';

    /** Differing bits — `BitVector` only. */
    public const METRIC_HAMMING = 'hamming';

    /** Set overlap normalised by set size — `BitVector` only. */
    public const METRIC_JACCARD = 'jaccard';

    /**
     * @param int $dimensions Vector width: float components for `Vector`, `HalfVector`
     *   and `SparseVector`, **bits** for `BitVector`. It sizes the column, and a query
     *   vector of a different width is refused rather than silently padded.
     * @param string $indexType One of the `INDEX_*` constants.
     * @param string $distanceMetric One of the `METRIC_*` constants.
     */
    public function __construct(
        public int $dimensions,
        public string $indexType = self::INDEX_HNSW,
        public string $distanceMetric = self::METRIC_COSINE,
    ) {
        if ($dimensions < 1) {
            throw new \InvalidArgumentException(sprintf(
                'A vector column has at least 1 dimension (got %d).',
                $dimensions,
            ));
        }
    }

    /**
     * The `vector_config` object as the AuthoringIR spells it.
     *
     * The index type and the metric are written out even where the author left them to
     * the default, so the emitted schema says which index and which metric the column
     * will get rather than leaving it to a compiler default the author cannot see.
     *
     * @return array{dimensions: int, index_type: string, distance_metric: string}
     */
    public function toArray(): array
    {
        return [
            'dimensions' => $this->dimensions,
            'index_type' => $this->indexType,
            'distance_metric' => $this->distanceMetric,
        ];
    }
}
