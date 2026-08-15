"""pgvector field authoring: `vector_config` and `vector_distance` (#959).

The compiler refuses a `Vector` field that carries no `vector_config`, so an SDK that
cannot author the config cannot author the type at all. These tests pin the whole
authoring path — annotation, field config, registry, exported `schema.json` — because
the defect this surface exists to prevent is a declaration that survives authoring and
disappears before the compiler sees it (#849, #852).
"""

from typing import Annotated

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID, BitVector, HalfVector, SparseVector, Vector
from fraiseql.schema import get_schema_dict


@pytest.fixture(autouse=True)
def clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def _fields(type_name: str) -> dict[str, dict]:
    types = {t["name"]: t for t in get_schema_dict()["types"]}
    return {f["name"]: f for f in types[type_name]["fields"]}


class TestVectorConfig:
    def test_dimensions_is_required_and_positive(self):
        with pytest.raises(ValueError, match="at least 1"):
            fraiseql.VectorConfig(dimensions=0)

    def test_index_and_metric_default_to_the_common_case(self):
        assert fraiseql.VectorConfig(dimensions=1536).to_dict() == {
            "dimensions": 1536,
            "index_type": "hnsw",
            "distance_metric": "cosine",
        }

    def test_a_field_is_a_vector_or_a_distance_but_not_both(self):
        with pytest.raises(ValueError, match="not both"):
            fraiseql.field(
                vector_config=fraiseql.VectorConfig(dimensions=8),
                vector_distance="embedding",
            )


class TestVectorFieldExport:
    def test_every_vector_field_type_carries_its_config_to_schema_json(self):
        """All four pgvector field types survive authoring with their own config.

        Each key is asserted, not just the object's presence: `index_type` and
        `distance_metric` both have compiler-side defaults, so a config that lost
        them would compile — to hnsw + cosine, chosen by nobody.
        """

        @fraiseql.type(sql_source="v_document")
        class Document:
            id: ID
            embedding: Annotated[
                Vector,
                fraiseql.field(
                    vector_config=fraiseql.VectorConfig(
                        dimensions=1536, index_type="ivf_flat", distance_metric="l2"
                    )
                ),
            ]
            fingerprint: Annotated[
                BitVector,
                fraiseql.field(
                    vector_config=fraiseql.VectorConfig(dimensions=768, distance_metric="hamming")
                ),
            ]
            compact: Annotated[
                HalfVector | None,
                fraiseql.field(
                    vector_config=fraiseql.VectorConfig(
                        dimensions=1536, distance_metric="inner_product"
                    )
                ),
            ] = None
            terms: Annotated[
                SparseVector | None,
                fraiseql.field(
                    vector_config=fraiseql.VectorConfig(dimensions=30000, index_type="none")
                ),
            ] = None

        fields = _fields("Document")
        assert fields["embedding"]["type"] == "Vector"
        assert fields["embedding"]["vector_config"] == {
            "dimensions": 1536,
            "index_type": "ivf_flat",
            "distance_metric": "l2",
        }
        assert fields["fingerprint"]["type"] == "BitVector"
        assert fields["fingerprint"]["vector_config"]["distance_metric"] == "hamming"
        assert fields["compact"]["type"] == "HalfVector"
        assert fields["compact"]["vector_config"]["distance_metric"] == "inner_product"
        assert fields["terms"]["type"] == "SparseVector"
        assert fields["terms"]["vector_config"]["index_type"] == "none"

    def test_a_distance_field_names_the_vector_it_measures(self):
        @fraiseql.type(sql_source="v_document")
        class Document:
            id: ID
            embedding: Annotated[
                Vector, fraiseql.field(vector_config=fraiseql.VectorConfig(dimensions=8))
            ]
            similarity: Annotated[float, fraiseql.field(vector_distance="embedding")]

        fields = _fields("Document")
        assert fields["similarity"]["type"] == "Float"
        assert fields["similarity"]["vector_distance"] == "embedding"

    def test_the_reference_follows_the_field_name_into_camel_case(self):
        """A snake_case vector field is exported camelCased, and so is the reference.

        Without this the author points at the name they wrote, the compiler looks for
        it among the camelCased field names, and the schema fails to compile naming a
        field that exists — under a different spelling.
        """

        @fraiseql.type(sql_source="v_document")
        class Document:
            id: ID
            title_embedding: Annotated[
                Vector, fraiseql.field(vector_config=fraiseql.VectorConfig(dimensions=8))
            ]
            similarity: Annotated[float, fraiseql.field(vector_distance="title_embedding")]

        fields = _fields("Document")
        assert "titleEmbedding" in fields
        assert fields["similarity"]["vector_distance"] == "titleEmbedding"
