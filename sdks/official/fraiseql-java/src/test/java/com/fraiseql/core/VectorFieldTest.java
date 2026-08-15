package com.fraiseql.core;

import com.fasterxml.jackson.databind.JsonNode;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * pgvector field authoring: {@code vector_config} and {@code vector_distance} (#959).
 *
 * <p>The compiler refuses a {@code Vector} field carrying no {@code vector_config}, so an
 * SDK that cannot author the config cannot author the type at all. These tests follow the
 * declaration all the way to the emitted JSON, because this SDK has already lost the
 * whole document between authoring and the compiler once (#851): the formatter's own
 * Javadoc claimed it produced the compiler's format while emitting a shape the compiler
 * rejected on the first key it read.
 */
@DisplayName("Java SDK pgvector field authoring")
public class VectorFieldTest {

    private SchemaRegistry registry;

    @BeforeEach
    void setUp() {
        registry = SchemaRegistry.getInstance();
        registry.clear();
    }

    private JsonNode field(String typeName, String fieldName) {
        JsonNode schema = SchemaFormatter.formatSchema(registry);
        for (JsonNode type : schema.get("types")) {
            if (!type.get("name").asText().equals(typeName)) {
                continue;
            }
            for (JsonNode field : type.get("fields")) {
                if (field.get("name").asText().equals(fieldName)) {
                    return field;
                }
            }
        }
        fail("field " + typeName + "." + fieldName + " is absent from the exported schema");
        return null;
    }

    @Test
    @DisplayName("Every vector field type carries its own config to schema.json")
    void everyVectorFieldTypeCarriesItsConfig() {
        FraiseQL.registerType(VectorDocument.class);

        // Every key is asserted, not just the object's presence: index_type and
        // distance_metric both have compiler-side defaults, so a config that lost them
        // would still compile — to hnsw + cosine, chosen by nobody.
        JsonNode embedding = field("VectorDocument", "embedding").get("vector_config");
        assertNotNull(embedding, "embedding carries no vector_config");
        assertEquals(1536, embedding.get("dimensions").asInt());
        assertEquals("ivf_flat", embedding.get("index_type").asText());
        assertEquals("l2", embedding.get("distance_metric").asText());

        assertEquals("hamming",
            field("VectorDocument", "fingerprint").get("vector_config").get("distance_metric").asText());
        assertEquals("inner_product",
            field("VectorDocument", "compact").get("vector_config").get("distance_metric").asText());
        assertEquals("none",
            field("VectorDocument", "terms").get("vector_config").get("index_type").asText());
    }

    @Test
    @DisplayName("The index and metric left to the default are written out")
    void theDefaultsAreWrittenOut() {
        FraiseQL.registerType(VectorDocument.class);

        JsonNode config = field("VectorDocument", "plain").get("vector_config");
        assertEquals(8, config.get("dimensions").asInt());
        assertEquals("hnsw", config.get("index_type").asText());
        assertEquals("cosine", config.get("distance_metric").asText());
    }

    @Test
    @DisplayName("A distance field names the vector it measures")
    void distanceFieldNamesTheVectorItMeasures() {
        FraiseQL.registerType(VectorDocument.class);

        assertEquals("embedding",
            field("VectorDocument", "similarity").get("vector_distance").asText());
    }

    @Test
    @DisplayName("An ordinary field carries no vector keys")
    void anOrdinaryFieldCarriesNoVectorKeys() {
        FraiseQL.registerType(VectorDocument.class);

        JsonNode id = field("VectorDocument", "id");
        assertFalse(id.has("vector_config"));
        assertFalse(id.has("vector_distance"));
    }

    @Test
    @DisplayName("A field is an embedding or a distance, not both")
    void aFieldIsAnEmbeddingOrADistanceNotBoth() {
        RuntimeException thrown = assertThrows(RuntimeException.class,
            () -> FraiseQL.registerType(BothVectorAndDistance.class));
        assertTrue(thrown.getMessage().contains("not both"), thrown.getMessage());
    }

    @Test
    @DisplayName("A dimension count no column can have is refused")
    void aDimensionCountNoColumnCanHaveIsRefused() {
        RuntimeException thrown = assertThrows(RuntimeException.class,
            () -> FraiseQL.registerType(NegativeDimensions.class));
        assertTrue(thrown.getMessage().contains("at least 1"), thrown.getMessage());
    }

    @GraphQLType(name = "VectorDocument", sqlSource = "v_document")
    public static class VectorDocument {
        @GraphQLField(type = "ID")
        public String id;

        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(
            dimensions = 1536,
            indexType = VectorConfig.INDEX_IVF_FLAT,
            distanceMetric = VectorConfig.METRIC_L2))
        public float[] embedding;

        @GraphQLField(type = Scalars.BIT_VECTOR, vector = @VectorConfig(
            dimensions = 768,
            distanceMetric = VectorConfig.METRIC_HAMMING))
        public String fingerprint;

        @GraphQLField(type = Scalars.HALF_VECTOR, nullable = true, vector = @VectorConfig(
            dimensions = 1536,
            distanceMetric = VectorConfig.METRIC_INNER_PRODUCT))
        public float[] compact;

        @GraphQLField(type = Scalars.SPARSE_VECTOR, nullable = true, vector = @VectorConfig(
            dimensions = 30000,
            indexType = VectorConfig.INDEX_NONE))
        public String terms;

        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(dimensions = 8))
        public float[] plain;

        @GraphQLField(type = "Float", vectorDistance = "embedding")
        public float similarity;
    }

    @GraphQLType(name = "BothVectorAndDistance", sqlSource = "v_document")
    public static class BothVectorAndDistance {
        @GraphQLField(type = Scalars.VECTOR,
            vector = @VectorConfig(dimensions = 8),
            vectorDistance = "embedding")
        public float[] embedding;
    }

    @GraphQLType(name = "NegativeDimensions", sqlSource = "v_document")
    public static class NegativeDimensions {
        @GraphQLField(type = Scalars.VECTOR, vector = @VectorConfig(dimensions = -1))
        public float[] embedding;
    }
}
