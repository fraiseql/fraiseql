/// `scoped_like_parent` authoring: a field that embeds a type whose own query is scoped by
/// an injected parameter declares it scoped like its parent. The compiler checks the
/// claim; the SDK's part is to carry the declaration, and only where it was made.
module FraiseQL.Tests.ScopedLikeParentTests

open System
open System.Text.Json
open Xunit
open FsUnit.Xunit
open FraiseQL
open FraiseQL.Dsl

[<GraphQLType(Name = "ScopedOrderLine", SqlSource = "v_order_line")>]
type ScopedOrderLine() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

[<GraphQLType(Name = "ScopedOrder", SqlSource = "v_order")>]
type ScopedOrder() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "[ScopedOrderLine!]", Nullable = false, ScopedLikeParent = true)>]
    member val Lines: ScopedOrderLine list = [] with get, set

    [<GraphQLField(Type = "ScopedOrderLine", Nullable = false)>]
    member val FirstLine = ScopedOrderLine() with get, set

[<GraphQLType(Name = "ScopedOrderInput", IsInput = true)>]
type ScopedOrderInput() =
    [<GraphQLField(Type = "ID", Nullable = false, ScopedLikeParent = true)>]
    member val LineId = "" with get, set

/// The fields of the type named `name` in the exported document, by name.
let private fields (json: string) (name: string) : Map<string, JsonElement> =
    use document = JsonDocument.Parse(json)

    document.RootElement.GetProperty("types").EnumerateArray()
    |> Seq.filter (fun t -> t.GetProperty("name").GetString() = name)
    |> Seq.collect (fun t -> t.GetProperty("fields").EnumerateArray())
    |> Seq.map (fun c -> c.GetProperty("name").GetString(), c.Clone())
    |> Map.ofSeq

let private hasKey (e: JsonElement) = e.TryGetProperty("scoped_like_parent") |> fst

let private emptySchema: IntermediateSchema =
    {
        version = "2.0.0"
        types = []
        input_types = []
        enums = []
        queries = []
        mutations = []
        functions = []
    }

[<Fact>]
let ``the key is exported only on the field that declares it`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<ScopedOrderLine>
    SchemaRegistry.register typeof<ScopedOrder>
    let json = SchemaExporter.export ()

    let order = fields json "ScopedOrder"
    order.["lines"].GetProperty("scoped_like_parent").GetBoolean() |> should equal true
    hasKey order.["firstLine"] |> should equal false
    hasKey order.["id"] |> should equal false
    hasKey (fields json "ScopedOrderLine").["id"] |> should equal false

[<Fact>]
let ``the DSL declares a scoped-like-parent field`` () =
    let line =
        TypeCEBuilder("OrderLine") {
            sqlSource "v_order_line"
            field (FieldBuilder("id", "ID") { nullable false })
        }

    let order =
        TypeCEBuilder("Order") {
            sqlSource "v_order"
            field (FieldBuilder("id", "ID") { nullable false })
            field (FieldBuilder("lines", "[OrderLine!]") { scopedLikeParent })
            field (FieldBuilder("firstLine", "OrderLine") { nullable false })
        }

    let json = SchemaExporter.exportSchemaCompact { emptySchema with types = [ line; order ] }

    let exported = fields json "Order"
    exported.["lines"].GetProperty("scoped_like_parent").GetBoolean() |> should equal true
    hasKey exported.["firstLine"] |> should equal false
    hasKey exported.["id"] |> should equal false
    hasKey (fields json "OrderLine").["id"] |> should equal false

[<Fact>]
let ``an input type's field cannot declare it`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<ScopedOrderInput>

    let refused =
        Assert.Throws<InvalidOperationException>(fun () -> SchemaExporter.export () |> ignore)

    refused.Message |> should haveSubstring "ScopedOrderInput.lineId"
    refused.Message |> should haveSubstring "output fields only"
    SchemaRegistry.reset ()

[<Fact>]
let ``the DSL's input type cannot declare it either`` () =
    let input =
        TypeCEBuilder("OrderInput") {
            sqlSource ""
            isInput true
            field (FieldBuilder("lineId", "ID") { scopedLikeParent })
        }

    let schema = { emptySchema with types = [ input ] }

    (fun () -> SchemaExporter.exportSchema schema |> ignore)
    |> should throw typeof<InvalidOperationException>

    (fun () -> SchemaExporter.exportSchemaCompact schema |> ignore)
    |> should throw typeof<InvalidOperationException>
