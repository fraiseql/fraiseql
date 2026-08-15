/// pgvector field authoring: `vector_config` and `vector_distance` (#959).
///
/// The compiler refuses a `Vector` field carrying no `vector_config`, so an SDK that
/// cannot author the config cannot author the type at all. These tests follow the
/// declaration all the way to the exported JSON, because the defect this surface exists
/// to prevent is one that survives authoring and disappears before the compiler sees it
/// — which is what #807 was here, for `requires_scope`.
module FraiseQL.Tests.VectorFieldTests

open System.Text.Json
open Xunit
open FsUnit.Xunit
open FraiseQL

[<GraphQLType(Name = "VectorDocument", SqlSource = "v_document")>]
type VectorDocument() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "Vector",
                   Nullable = false,
                   VectorDimensions = 1536,
                   VectorIndexType = VectorIndex.ivfFlat,
                   VectorDistanceMetric = VectorMetric.l2)>]
    member val Embedding: float array = [||] with get, set

    [<GraphQLField(Type = "BitVector",
                   Nullable = false,
                   VectorDimensions = 768,
                   VectorDistanceMetric = VectorMetric.hamming)>]
    member val Fingerprint = "" with get, set

    [<GraphQLField(Type = "SparseVector",
                   Nullable = true,
                   VectorDimensions = 30000,
                   VectorIndexType = VectorIndex.none)>]
    member val Terms = "" with get, set

    [<GraphQLField(Type = "Vector", Nullable = false, VectorDimensions = 8)>]
    member val Plain: float array = [||] with get, set

    [<GraphQLField(Type = "Float", Nullable = false, VectorDistance = "titleEmbedding")>]
    member val Similarity = 0.0 with get, set

[<GraphQLType(Name = "BothVectorAndDistance", SqlSource = "v_document")>]
type BothVectorAndDistance() =
    [<GraphQLField(Type = "Vector",
                   Nullable = false,
                   VectorDimensions = 8,
                   VectorDistance = "embedding")>]
    member val Embedding: float array = [||] with get, set

[<GraphQLType(Name = "NoDimensions", SqlSource = "v_document")>]
type NoDimensions() =
    [<GraphQLField(Type = "Vector", Nullable = false, VectorDistanceMetric = VectorMetric.l2)>]
    member val Embedding: float array = [||] with get, set

let private field (typeName: string) (fieldName: string) : JsonElement =
    use document = JsonDocument.Parse(SchemaExporter.export ())

    let types =
        document.RootElement.GetProperty("types").EnumerateArray()
        |> Seq.filter (fun t -> t.GetProperty("name").GetString() = typeName)

    let matches =
        types
        |> Seq.collect (fun t -> t.GetProperty("fields").EnumerateArray())
        |> Seq.filter (fun f -> f.GetProperty("name").GetString() = fieldName)
        |> Seq.map (fun f -> f.Clone())
        |> Seq.toList

    match matches with
    | [ f ] -> f
    | _ -> failwithf "field %s.%s is absent from the export" typeName fieldName

let private register () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<VectorDocument>

[<Fact>]
let ``every vector field type carries its own config`` () =
    register ()

    // Every key is asserted, not just the object's presence: index_type and
    // distance_metric both have compiler-side defaults, so a config that lost them
    // would still compile — to hnsw + cosine, chosen by nobody.
    let embedding = (field "VectorDocument" "embedding").GetProperty("vector_config")
    embedding.GetProperty("dimensions").GetInt32() |> should equal 1536
    embedding.GetProperty("index_type").GetString() |> should equal "ivf_flat"
    embedding.GetProperty("distance_metric").GetString() |> should equal "l2"

    (field "VectorDocument" "fingerprint")
        .GetProperty("vector_config")
        .GetProperty("distance_metric")
        .GetString()
    |> should equal "hamming"

    (field "VectorDocument" "terms")
        .GetProperty("vector_config")
        .GetProperty("index_type")
        .GetString()
    |> should equal "none"

[<Fact>]
let ``the index and metric left to the default are written out`` () =
    register ()

    let plain = (field "VectorDocument" "plain").GetProperty("vector_config")
    plain.GetProperty("dimensions").GetInt32() |> should equal 8
    plain.GetProperty("index_type").GetString() |> should equal "hnsw"
    plain.GetProperty("distance_metric").GetString() |> should equal "cosine"

[<Fact>]
let ``a distance reference follows the field name into snake_case`` () =
    // Field names are snake_cased on the way out, so the reference has to be too, or
    // the author points at the name they wrote and the compiler looks for it among
    // names spelled differently.
    register ()

    (field "VectorDocument" "similarity").GetProperty("vector_distance").GetString()
    |> should equal "title_embedding"

[<Fact>]
let ``an ordinary field carries no vector keys`` () =
    register ()

    let id = field "VectorDocument" "id"
    fst (id.TryGetProperty("vector_config")) |> should equal false
    fst (id.TryGetProperty("vector_distance")) |> should equal false

[<Fact>]
let ``a field is an embedding or a distance, not both`` () =
    SchemaRegistry.reset ()

    let thrown =
        Assert.Throws<System.ArgumentException>(fun () ->
            SchemaRegistry.register typeof<BothVectorAndDistance>)

    thrown.Message |> should haveSubstring "not both"

[<Fact>]
let ``a dimension count no column can have is refused`` () =
    SchemaRegistry.reset ()

    let thrown =
        Assert.Throws<System.ArgumentException>(fun () -> SchemaRegistry.register typeof<NoDimensions>)

    thrown.Message |> should haveSubstring "at least 1"
