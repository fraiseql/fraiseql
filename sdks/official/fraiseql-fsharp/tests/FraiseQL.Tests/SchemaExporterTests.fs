module FraiseQL.Tests.SchemaExporterTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let private emptySchema: IntermediateSchema =
    {
        version = "2.0.0"
        types = []
        queries = []
        mutations = []
    }

let private singleField =
    {
        name = "id"
        type_ = "ID"
        nullable = false
        description = None
        scope = None
    }

let private singleType =
    {
        name = "Author"
        sql_source = "v_author"
        description = None
        fields = [ singleField ]
        is_input = false
        relay = false
        is_error = false
    }

let private singleQuery: QueryDefinition =
    {
        name = "authors"
        return_type = "Author"
        returns_list = true
        nullable = false
        sql_source = "v_author"
        arguments = []
        cache_ttl_seconds = None
        description = None
        rest = None
    }

let private singleMutation: MutationDefinition =
    {
        name = "createAuthor"
        return_type = "Author"
        sql_source = "fn_create_author"
        operation = "insert"
        arguments = []
        description = None
        rest = None
        cascade = None
    }

let private parseJson (json: string) =
    System.Text.Json.JsonDocument.Parse(json).RootElement

// ---------------------------------------------------------------------------
// Empty schema tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``export empty schema contains version 2.0.0`` () =
    let json = SchemaExporter.exportSchema emptySchema
    let root = parseJson json
    root.GetProperty("version").GetString() |> should equal "2.0.0"

[<Fact>]
let ``export empty schema has empty types array`` () =
    let json = SchemaExporter.exportSchema emptySchema
    let root = parseJson json
    root.GetProperty("types").GetArrayLength() |> should equal 0

[<Fact>]
let ``export empty schema has empty queries array`` () =
    let json = SchemaExporter.exportSchema emptySchema
    let root = parseJson json
    root.GetProperty("queries").GetArrayLength() |> should equal 0

[<Fact>]
let ``export empty schema has empty mutations array`` () =
    let json = SchemaExporter.exportSchema emptySchema
    let root = parseJson json
    root.GetProperty("mutations").GetArrayLength() |> should equal 0

// ---------------------------------------------------------------------------
// Type serialization tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``export type uses snake_case key sql_source`` () =
    let schema = { emptySchema with types = [ singleType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    let t = root.GetProperty("types").[0]
    t.GetProperty("sql_source").GetString() |> should equal "v_author"

[<Fact>]
let ``export type serializes name`` () =
    let schema = { emptySchema with types = [ singleType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("types").[0].GetProperty("name").GetString() |> should equal "Author"

[<Fact>]
let ``export field uses key type not type_`` () =
    let schema = { emptySchema with types = [ singleType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    let field = root.GetProperty("types").[0].GetProperty("fields").[0]
    field.GetProperty("type").GetString() |> should equal "ID"

[<Fact>]
let ``export field serializes nullable false`` () =
    let schema = { emptySchema with types = [ singleType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    let field = root.GetProperty("types").[0].GetProperty("fields").[0]
    field.GetProperty("nullable").GetBoolean() |> should equal false

[<Fact>]
let ``export field omits null description`` () =
    let schema = { emptySchema with types = [ singleType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    let field = root.GetProperty("types").[0].GetProperty("fields").[0]

    field.TryGetProperty("description") |> fst |> should equal false

[<Fact>]
let ``export field includes description when present`` () =
    let fieldWithDesc = { singleField with description = Some "The author ID" }
    let typeWithDesc = { singleType with fields = [ fieldWithDesc ] }
    let schema = { emptySchema with types = [ typeWithDesc ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    let field = root.GetProperty("types").[0].GetProperty("fields").[0]
    field.GetProperty("description").GetString() |> should equal "The author ID"

[<Fact>]
let ``export type serializes is_input`` () =
    let inputType = { singleType with is_input = true }
    let schema = { emptySchema with types = [ inputType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("types").[0].GetProperty("is_input").GetBoolean() |> should equal true

[<Fact>]
let ``export type serializes relay`` () =
    let relayType = { singleType with relay = true }
    let schema = { emptySchema with types = [ relayType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("types").[0].GetProperty("relay").GetBoolean() |> should equal true

[<Fact>]
let ``export type serializes is_error`` () =
    let errorType = { singleType with is_error = true }
    let schema = { emptySchema with types = [ errorType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("types").[0].GetProperty("is_error").GetBoolean() |> should equal true

[<Fact>]
let ``export type omits null description`` () =
    let schema = { emptySchema with types = [ singleType ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("types").[0].TryGetProperty("description") |> fst |> should equal false

[<Fact>]
let ``export type includes description when present`` () =
    let typeWithDesc = { singleType with description = Some "Blog authors" }
    let schema = { emptySchema with types = [ typeWithDesc ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("types").[0].GetProperty("description").GetString()
    |> should equal "Blog authors"

// ---------------------------------------------------------------------------
// Query serialization tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``export query uses snake_case key return_type`` () =
    let schema = { emptySchema with queries = [ singleQuery ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("queries").[0].GetProperty("return_type").GetString()
    |> should equal "Author"

[<Fact>]
let ``export query uses snake_case key returns_list`` () =
    let schema = { emptySchema with queries = [ singleQuery ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("queries").[0].GetProperty("returns_list").GetBoolean()
    |> should equal true

[<Fact>]
let ``export query uses snake_case key sql_source`` () =
    let schema = { emptySchema with queries = [ singleQuery ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("queries").[0].GetProperty("sql_source").GetString()
    |> should equal "v_author"

[<Fact>]
let ``export query omits null cache_ttl_seconds`` () =
    let schema = { emptySchema with queries = [ singleQuery ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json

    root.GetProperty("queries").[0].TryGetProperty("cache_ttl_seconds")
    |> fst
    |> should equal false

[<Fact>]
let ``export query includes cache_ttl_seconds when present`` () =
    let cachedQuery = { singleQuery with cache_ttl_seconds = Some 300 }
    let schema = { emptySchema with queries = [ cachedQuery ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("queries").[0].GetProperty("cache_ttl_seconds").GetInt32()
    |> should equal 300

[<Fact>]
let ``export query includes argument with type key`` () =
    let arg: ArgumentDefinition = { name = "id"; type_ = "ID"; nullable = false }

    let queryWithArg = { singleQuery with arguments = [ arg ] }
    let schema = { emptySchema with queries = [ queryWithArg ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    let argEl = root.GetProperty("queries").[0].GetProperty("arguments").[0]
    argEl.GetProperty("type").GetString() |> should equal "ID"
    argEl.GetProperty("name").GetString() |> should equal "id"

// ---------------------------------------------------------------------------
// Mutation serialization tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``export mutation uses snake_case key return_type`` () =
    let schema = { emptySchema with mutations = [ singleMutation ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("mutations").[0].GetProperty("return_type").GetString()
    |> should equal "Author"

[<Fact>]
let ``export mutation uses snake_case key sql_source`` () =
    let schema = { emptySchema with mutations = [ singleMutation ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("mutations").[0].GetProperty("sql_source").GetString()
    |> should equal "fn_create_author"

[<Fact>]
let ``export mutation serializes operation`` () =
    let schema = { emptySchema with mutations = [ singleMutation ] }
    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("mutations").[0].GetProperty("operation").GetString()
    |> should equal "insert"

// ---------------------------------------------------------------------------
// Compact export tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``exportSchemaCompact produces valid JSON`` () =
    let schema = { emptySchema with types = [ singleType ]; queries = [ singleQuery ] }
    let json = SchemaExporter.exportSchemaCompact schema
    json.Contains("\n") |> should equal false
    parseJson json |> ignore

// ---------------------------------------------------------------------------
// File export tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``exportSchemaToFile writes file to disk`` () =
    let path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), Guid.NewGuid().ToString() + ".json")

    try
        SchemaExporter.exportSchemaToFile path emptySchema
        System.IO.File.Exists(path) |> should equal true
    finally
        if System.IO.File.Exists(path) then
            System.IO.File.Delete(path)

[<Fact>]
let ``exportSchemaToFile creates missing directories`` () =
    let dir =
        System.IO.Path.Combine(System.IO.Path.GetTempPath(), Guid.NewGuid().ToString(), "subdir")

    let path = System.IO.Path.Combine(dir, "schema.json")

    try
        SchemaExporter.exportSchemaToFile path emptySchema
        System.IO.File.Exists(path) |> should equal true
    finally
        if System.IO.Directory.Exists(dir) then
            System.IO.Directory.Delete(dir, true)

[<Fact>]
let ``exported file contains valid JSON`` () =
    let path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), Guid.NewGuid().ToString() + ".json")

    try
        let schema = { emptySchema with types = [ singleType ] }
        SchemaExporter.exportSchemaToFile path schema
        let content = System.IO.File.ReadAllText(path)
        parseJson content |> ignore
    finally
        if System.IO.File.Exists(path) then
            System.IO.File.Delete(path)

// ---------------------------------------------------------------------------
// Registry-based export tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``export from registry after reset produces empty schema`` () =
    SchemaRegistry.reset ()
    let json = SchemaExporter.export ()
    let root = parseJson json
    root.GetProperty("types").GetArrayLength() |> should equal 0
