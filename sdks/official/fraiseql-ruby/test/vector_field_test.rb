# frozen_string_literal: true

require "test_helper"

# pgvector field authoring: `vector_config` and `vector_distance` (#959).
#
# The compiler refuses a `Vector` field carrying no `vector_config`, so an SDK that
# cannot author the config cannot author the type at all. These tests follow the
# declaration all the way to the exported hash, because this SDK has already shipped a
# README documenting an exporter that did not exist and a `to_fraiseql_schema` that
# omitted a required key (#853, #854) — the whole class of "authored, then lost".
class VectorFieldTest < Minitest::Test
  def document_fields
    schema = FraiseQL::Schema.new
    schema.type "Document", sql_source: "v_document" do |t|
      t.field :id, :id, nullable: false
      t.field :embedding, :vector, nullable: false,
                                   vector_config: { dimensions: 1536, index_type: :ivf_flat,
                                                    distance_metric: :l2 }
      t.field :fingerprint, :bit_vector, nullable: false,
                                         vector_config: { dimensions: 768,
                                                          distance_metric: :hamming }
      t.field :compact, :half_vector, nullable: true,
                                      vector_config: { dimensions: 1536,
                                                       distance_metric: :inner_product }
      t.field :terms, :sparse_vector, nullable: true,
                                      vector_config: { dimensions: 30_000, index_type: :none }
      t.field :plain, :vector, nullable: false, vector_config: { dimensions: 8 }
      t.field :similarity, :float, nullable: false, vector_distance: :embedding
    end

    schema.to_h["types"].first["fields"].each_with_object({}) { |f, acc| acc[f["name"]] = f }
  end

  def test_the_four_pgvector_field_types_keep_their_own_type_name
    fields = document_fields

    assert_equal "Vector", fields["embedding"]["type"]
    assert_equal "BitVector", fields["fingerprint"]["type"]
    assert_equal "HalfVector", fields["compact"]["type"]
    assert_equal "SparseVector", fields["terms"]["type"]
  end

  def test_every_key_of_vector_config_survives
    # Every key is asserted, not just the object's presence: index_type and
    # distance_metric both have compiler-side defaults, so a config that lost them would
    # still compile — to hnsw + cosine, chosen by nobody.
    fields = document_fields

    assert_equal({ "dimensions" => 1536, "index_type" => "ivf_flat", "distance_metric" => "l2" },
                 fields["embedding"]["vector_config"])
    assert_equal "hamming", fields["fingerprint"]["vector_config"]["distance_metric"]
    assert_equal "inner_product", fields["compact"]["vector_config"]["distance_metric"]
    assert_equal "none", fields["terms"]["vector_config"]["index_type"]
  end

  def test_the_index_and_metric_left_to_the_default_are_written_out
    assert_equal({ "dimensions" => 8, "index_type" => "hnsw", "distance_metric" => "cosine" },
                 document_fields["plain"]["vector_config"])
  end

  def test_a_distance_field_names_the_vector_it_measures
    assert_equal "embedding", document_fields["similarity"]["vector_distance"]
  end

  def test_an_ordinary_field_carries_no_vector_keys
    id = document_fields["id"]

    refute id.key?("vector_config")
    refute id.key?("vector_distance")
  end

  def test_a_field_is_an_embedding_or_a_distance_not_both
    error = assert_raises(ArgumentError) do
      FraiseQL::Schema.new.type "Document", sql_source: "v_document" do |t|
        t.field :embedding, :vector, nullable: false,
                                     vector_config: { dimensions: 8 },
                                     vector_distance: :embedding
      end
    end

    assert_match(/not both/, error.message)
  end

  def test_a_dimension_count_no_column_can_have_is_refused
    error = assert_raises(ArgumentError) do
      FraiseQL::Schema.new.type "Document", sql_source: "v_document" do |t|
        t.field :embedding, :vector, nullable: false, vector_config: { dimensions: 0 }
      end
    end

    assert_match(/at least 1/, error.message)
  end
end
