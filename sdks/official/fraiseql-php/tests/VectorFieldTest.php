<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use FraiseQL\VectorConfig;
use PHPUnit\Framework\TestCase;

/**
 * pgvector field authoring: `vector_config` and `vector_distance` (#959).
 *
 * The compiler refuses a `Vector` field carrying no `vector_config`, so an SDK that
 * cannot author the config cannot author the type at all. These tests follow the
 * declaration all the way to the exported JSON, because this SDK has already lost a
 * field-level declaration between the attribute and the compiler twice — #807's scope
 * was dropped by the exporter, and the constructor that was meant to carry it never
 * passed it on, so the fix one layer up could not have worked.
 */
final class VectorFieldTest extends TestCase
{
    private SchemaRegistry $registry;

    protected function setUp(): void
    {
        $this->registry = SchemaRegistry::getInstance();
        $this->registry->clear();
    }

    protected function tearDown(): void
    {
        $this->registry->clear();
    }

    /**
     * @return array<string, array<string, mixed>>
     */
    private function exportedFields(string $typeName): array
    {
        $schema = SchemaExporter::toArray();
        foreach ($schema['types'] as $type) {
            if ($type['name'] === $typeName) {
                $fields = [];
                foreach ($type['fields'] as $field) {
                    $fields[$field['name']] = $field;
                }

                return $fields;
            }
        }

        self::fail(sprintf('type "%s" is absent from the exported schema', $typeName));
    }

    public function testEveryVectorFieldTypeCarriesItsConfig(): void
    {
        StaticAPI::register(VectorDocument::class);

        $fields = $this->exportedFields('VectorDocument');

        // Every key is asserted, not just the object's presence: `index_type` and
        // `distance_metric` both have compiler-side defaults, so a config that lost
        // them would still compile — to hnsw + cosine, chosen by nobody.
        self::assertSame([
            'dimensions' => 1536,
            'index_type' => 'ivf_flat',
            'distance_metric' => 'l2',
        ], $fields['embedding']['vector_config']);
        self::assertSame('hamming', $fields['fingerprint']['vector_config']['distance_metric']);
        self::assertSame('inner_product', $fields['compact']['vector_config']['distance_metric']);
        self::assertSame('none', $fields['terms']['vector_config']['index_type']);
    }

    public function testDistanceFieldNamesTheVectorItMeasures(): void
    {
        StaticAPI::register(VectorDocument::class);

        self::assertSame('embedding', $this->exportedFields('VectorDocument')['similarity']['vector_distance']);
    }

    public function testAnOrdinaryFieldCarriesNoVectorKeys(): void
    {
        StaticAPI::register(VectorDocument::class);

        $id = $this->exportedFields('VectorDocument')['id'];
        self::assertArrayNotHasKey('vector_config', $id);
        self::assertArrayNotHasKey('vector_distance', $id);
    }

    public function testTheIndexAndMetricLeftToTheDefaultAreWrittenOut(): void
    {
        self::assertSame([
            'dimensions' => 8,
            'index_type' => 'hnsw',
            'distance_metric' => 'cosine',
        ], (new VectorConfig(8))->toArray());
    }

    public function testADimensionCountNoColumnCanHaveIsRefused(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/at least 1 dimension/');

        new VectorConfig(0);
    }

    public function testAFieldIsAnEmbeddingOrADistanceNotBoth(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/not both/');

        new GraphQLField(
            type: 'Vector',
            vectorConfig: new VectorConfig(8),
            vectorDistance: 'embedding',
        );
    }
}

#[GraphQLType(name: 'VectorDocument', sqlSource: 'v_document')]
final class VectorDocument
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'Vector', nullable: false, vectorConfig: new VectorConfig(
        dimensions: 1536,
        indexType: VectorConfig::INDEX_IVF_FLAT,
        distanceMetric: VectorConfig::METRIC_L2,
    ))]
    public array $embedding;

    #[GraphQLField(type: 'BitVector', nullable: false, vectorConfig: new VectorConfig(
        dimensions: 768,
        distanceMetric: VectorConfig::METRIC_HAMMING,
    ))]
    public string $fingerprint;

    #[GraphQLField(type: 'HalfVector', nullable: true, vectorConfig: new VectorConfig(
        dimensions: 1536,
        distanceMetric: VectorConfig::METRIC_INNER_PRODUCT,
    ))]
    public ?array $compact;

    #[GraphQLField(type: 'SparseVector', nullable: true, vectorConfig: new VectorConfig(
        dimensions: 30000,
        indexType: VectorConfig::INDEX_NONE,
    ))]
    public ?string $terms;

    #[GraphQLField(type: 'Float', nullable: false, vectorDistance: 'embedding')]
    public float $similarity;
}
