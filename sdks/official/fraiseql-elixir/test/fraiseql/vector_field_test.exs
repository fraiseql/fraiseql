defmodule FraiseQL.VectorFieldTest do
  @moduledoc """
  pgvector field authoring: `vector_config` and `vector_distance` (#959).

  The compiler refuses a `Vector` field carrying no `vector_config`, so an SDK that
  cannot author the config cannot author the type at all. These tests follow the
  declaration all the way to the exported JSON, because the defect this surface exists to
  prevent is one that survives authoring and disappears before the compiler sees it —
  which is what #807 was here, for `requires_scope`.
  """
  use ExUnit.Case

  defmodule VectorSchema do
    use FraiseQL.Schema

    fraiseql_type "Document", sql_source: "v_document" do
      field :id, :id, nullable: false

      field :embedding, :vector,
        nullable: false,
        vector_config: [dimensions: 1536, index_type: :ivf_flat, distance_metric: :l2]

      field :fingerprint, :bit_vector,
        nullable: false,
        vector_config: [dimensions: 768, distance_metric: :hamming]

      field :compact, :half_vector,
        nullable: true,
        vector_config: [dimensions: 1536, distance_metric: :inner_product]

      field :terms, :sparse_vector,
        nullable: true,
        vector_config: [dimensions: 30_000, index_type: :none]

      field :plain, :vector, nullable: false, vector_config: [dimensions: 8]

      field :similarity, :float, nullable: false, vector_distance: :embedding
    end
  end

  defp fields do
    VectorSchema
    |> FraiseQL.SchemaExporter.export()
    |> Jason.decode!()
    |> Map.fetch!("types")
    |> Enum.find(&(&1["name"] == "Document"))
    |> Map.fetch!("fields")
    |> Map.new(&{&1["name"], &1})
  end

  test "the four pgvector field types keep their own type name" do
    fields = fields()
    assert fields["embedding"]["type"] == "Vector"
    assert fields["fingerprint"]["type"] == "BitVector"
    assert fields["compact"]["type"] == "HalfVector"
    assert fields["terms"]["type"] == "SparseVector"
  end

  test "every key of vector_config survives to schema.json" do
    # Every key is asserted, not just the object's presence: index_type and
    # distance_metric both have compiler-side defaults, so a config that lost them would
    # still compile — to hnsw + cosine, chosen by nobody.
    fields = fields()

    assert fields["embedding"]["vector_config"] == %{
             "dimensions" => 1536,
             "index_type" => "ivf_flat",
             "distance_metric" => "l2"
           }

    assert fields["fingerprint"]["vector_config"]["distance_metric"] == "hamming"
    assert fields["compact"]["vector_config"]["distance_metric"] == "inner_product"
    assert fields["terms"]["vector_config"]["index_type"] == "none"
  end

  test "the index and metric left to the default are written out" do
    assert fields()["plain"]["vector_config"] == %{
             "dimensions" => 8,
             "index_type" => "hnsw",
             "distance_metric" => "cosine"
           }
  end

  test "a distance field names the vector it measures" do
    assert fields()["similarity"]["vector_distance"] == "embedding"
  end

  test "an ordinary field carries no vector keys" do
    id = fields()["id"]
    refute Map.has_key?(id, "vector_config")
    refute Map.has_key?(id, "vector_distance")
  end

  test "a field is an embedding or a distance, not both" do
    assert_raise ArgumentError, ~r/not both/, fn ->
      defmodule BothVectorAndDistance do
        use FraiseQL.Schema

        fraiseql_type "Both", sql_source: "v_document" do
          field :embedding, :vector,
            nullable: false,
            vector_config: [dimensions: 8],
            vector_distance: :embedding
        end
      end
    end
  end

  test "a dimension count no column can have is refused" do
    assert_raise ArgumentError, ~r/at least 1/, fn ->
      defmodule NoDimensions do
        use FraiseQL.Schema

        fraiseql_type "NoDimensions", sql_source: "v_document" do
          field :embedding, :vector, nullable: false, vector_config: [dimensions: 0]
        end
      end
    end
  end
end
