/// `localized` authoring (#1527): a String field, input field or mutation argument stored
/// as a locale map. These tests follow the declaration to the exported JSON, and refuse it
/// on anything but a String at export, as the Python and TypeScript SDKs refuse
/// `Localized[int]`.
module FraiseQL.Tests.LocalizedTests

open System
open System.Text.Json
open Xunit
open FsUnit.Xunit
open FraiseQL
open FraiseQL.Dsl

[<GraphQLType(Name = "LocalizedProduct", SqlSource = "v_product")>]
type LocalizedProduct() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "String", Nullable = true, Localized = true)>]
    member val Name = "" with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Sku = "" with get, set

[<GraphQLType(Name = "LocalizedPriced", SqlSource = "v_priced")>]
type LocalizedPriced() =
    [<GraphQLField(Type = "Float", Nullable = false, Localized = true)>]
    member val Price = 0.0 with get, set

[<GraphQLType(Name = "LocalizedArticle", SqlSource = "v_article", Crud = true)>]
type LocalizedArticle() =
    [<GraphQLField(Type = "Int", Nullable = false)>]
    member val Id = 0 with get, set

    [<GraphQLField(Type = "String", Nullable = false, Localized = true)>]
    member val Title = "" with get, set

/// The objects of the root's `section` named `name`: their `key` array, by name.
let private named (section: string) (name: string) (key: string) : Map<string, JsonElement> =
    use document = JsonDocument.Parse(SchemaExporter.export ())

    document.RootElement.GetProperty(section).EnumerateArray()
    |> Seq.filter (fun t -> t.GetProperty("name").GetString() = name)
    |> Seq.collect (fun t -> t.GetProperty(key).EnumerateArray())
    |> Seq.map (fun c -> c.GetProperty("name").GetString(), c.Clone())
    |> Map.ofSeq

let private isLocalized (e: JsonElement) =
    match e.TryGetProperty("localized") with
    | true, flag -> flag.GetBoolean()
    | _ -> false

[<Fact>]
let ``a localized field and input field are exported as localized`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<LocalizedProduct>

    SchemaRegistry.registerInput
        {
            name = "CreateProductInput"
            fields = [ { name = "name"; type_ = "String"; nullable = false; localized = Some true } ]
            description = None
        }

    let fields = named "types" "LocalizedProduct" "fields"
    isLocalized fields.["name"] |> should equal true
    fields.["sku"].TryGetProperty("localized") |> fst |> should equal false
    isLocalized (named "input_types" "CreateProductInput" "fields").["name"] |> should equal true

[<Fact>]
let ``a localized mutation argument is exported as a localized String`` () =
    SchemaRegistry.reset ()

    MutationBuilder.mutation "createProduct"
    |> MutationBuilder.returnType "Product"
    |> MutationBuilder.sqlSource "fn_create_product"
    |> MutationBuilder.operation "insert"
    |> MutationBuilder.withArgument "sku" "String" false
    |> MutationBuilder.withLocalizedArgument "name" true
    |> MutationBuilder.register

    let args = named "mutations" "createProduct" "arguments"
    args.["name"].GetProperty("type").GetString() |> should equal "String"
    args.["name"].GetProperty("nullable").GetBoolean() |> should equal true
    isLocalized args.["name"] |> should equal true
    args.["sku"].TryGetProperty("localized") |> fst |> should equal false

[<Fact>]
let ``the DSL declares a localized field and argument`` () =
    let field = FieldBuilder("name", "String") { localized }
    field.localized |> should equal (Some true)

    let mutation =
        MutationCEBuilder("createProduct") {
            returnType "Product"
            sqlSource "fn_create_product"
            localizedArg "name" true
        }

    mutation.arguments
    |> should equal [ { name = "name"; type_ = "String"; nullable = true; localized = Some true } ]

[<Fact>]
let ``localized is refused on a field that is not a String`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<LocalizedPriced>

    let refused = Assert.Throws<InvalidOperationException>(fun () -> SchemaExporter.export () |> ignore)
    refused.Message |> should haveSubstring "price"
    refused.Message |> should haveSubstring "only a String can be localized"

[<Fact>]
let ``localized is refused on a non-String input field and argument`` () =
    SchemaRegistry.reset ()

    SchemaRegistry.registerInput
        {
            name = "PriceInput"
            fields = [ { name = "amount"; type_ = "Float"; nullable = false; localized = Some true } ]
            description = None
        }

    (Assert.Throws<InvalidOperationException>(fun () -> SchemaExporter.export () |> ignore)).Message
    |> should haveSubstring "amount"

    SchemaRegistry.reset ()

    MutationBuilder.mutation "setPrice"
    |> MutationBuilder.returnType "Product"
    |> MutationBuilder.sqlSource "fn_set_price"
    |> MutationBuilder.operation "update"
    |> fun s ->
        { s with arguments = [ { name = "amount"; type_ = "Float"; nullable = false; localized = Some true } ] }
    |> MutationBuilder.register

    (Assert.Throws<InvalidOperationException>(fun () -> SchemaExporter.export () |> ignore)).Message
    |> should haveSubstring "amount"

    SchemaRegistry.reset ()

[<Fact>]
let ``a CRUD input field is localized as its field is`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<LocalizedArticle>

    isLocalized (named "input_types" "CreateLocalizedArticleInput" "fields").["title"]
    |> should equal true

    isLocalized (named "input_types" "UpdateLocalizedArticleInput" "fields").["title"]
    |> should equal true

    SchemaRegistry.reset ()

[<Fact>]
let ``the compact export refuses a localized non-String too`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<LocalizedPriced>

    Assert.Throws<InvalidOperationException>(fun () ->
        SchemaExporter.exportSchemaCompact (SchemaRegistry.toIntermediateSchema ()) |> ignore)
    |> ignore

    SchemaRegistry.reset ()
