module FraiseQL.Tests.DslTests

open Xunit
open FsUnit.Xunit
open FraiseQL
open FraiseQL.Dsl

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let private parseJson (json: string) =
    System.Text.Json.JsonDocument.Parse(json).RootElement

// ---------------------------------------------------------------------------
// Empty schema
// ---------------------------------------------------------------------------

[<Fact>]
let ``empty fraiseql CE produces version 2.0.0`` () =
    let schema = fraiseql { () }
    schema.version |> should equal "2.0.0"

[<Fact>]
let ``empty fraiseql CE produces empty types`` () =
    let schema = fraiseql { () }
    schema.types |> should be Empty

[<Fact>]
let ``empty fraiseql CE produces empty queries`` () =
    let schema = fraiseql { () }
    schema.queries |> should be Empty

[<Fact>]
let ``empty fraiseql CE produces empty mutations`` () =
    let schema = fraiseql { () }
    schema.mutations |> should be Empty

// ---------------------------------------------------------------------------
// FieldBuilder
// ---------------------------------------------------------------------------

[<Fact>]
let ``FieldBuilder produces correct name and type`` () =
    let f: FieldDefinition = FieldBuilder("id", "ID") { () }
    f.name |> should equal "id"
    f.type_ |> should equal "ID"

[<Fact>]
let ``FieldBuilder defaults to nullable true`` () =
    let f: FieldDefinition = FieldBuilder("bio", "String") { () }
    f.nullable |> should equal true

[<Fact>]
let ``FieldBuilder nullable false via custom operation`` () =
    let f: FieldDefinition = FieldBuilder("id", "ID") { nullable false }
    f.nullable |> should equal false

[<Fact>]
let ``FieldBuilder description via custom operation`` () =
    let f: FieldDefinition = FieldBuilder("id", "ID") { description "The ID" }
    f.description |> should equal (Some "The ID")

[<Fact>]
let ``FieldBuilder scope via custom operation`` () =
    let f: FieldDefinition = FieldBuilder("secret", "String") { scope "read:admin" }
    f.scope |> should equal (Some "read:admin")

// ---------------------------------------------------------------------------
// TypeCEBuilder
// ---------------------------------------------------------------------------

[<Fact>]
let ``TypeCEBuilder produces correct name`` () =
    let td = TypeCEBuilder("Author") { sqlSource "v_author" }
    td.name |> should equal "Author"

[<Fact>]
let ``TypeCEBuilder sqlSource via custom operation`` () =
    let td = TypeCEBuilder("Author") { sqlSource "v_author" }
    td.sql_source |> should equal "v_author"

[<Fact>]
let ``TypeCEBuilder description via custom operation`` () =
    let td = TypeCEBuilder("Author") { sqlSource "v_author"; description "Blog author" }
    td.description |> should equal (Some "Blog author")

[<Fact>]
let ``TypeCEBuilder field adds FieldDefinition`` () =
    let td =
        TypeCEBuilder("Author") {
            sqlSource "v_author"
            field (FieldBuilder("id", "ID") { nullable false })
        }

    td.fields |> List.length |> should equal 1
    td.fields.[0].name |> should equal "id"

[<Fact>]
let ``TypeCEBuilder multiple fields`` () =
    let td =
        TypeCEBuilder("Author") {
            sqlSource "v_author"
            field (FieldBuilder("id", "ID") { nullable false })
            field (FieldBuilder("name", "String") { nullable false })
        }

    td.fields |> List.length |> should equal 2

[<Fact>]
let ``TypeCEBuilder isInput via custom operation`` () =
    let td = TypeCEBuilder("CreateAuthorInput") { sqlSource ""; isInput true }
    td.is_input |> should equal true

[<Fact>]
let ``TypeCEBuilder relay via custom operation`` () =
    let td = TypeCEBuilder("Post") { sqlSource "v_post"; relay true }
    td.relay |> should equal true

[<Fact>]
let ``TypeCEBuilder isError via custom operation`` () =
    let td = TypeCEBuilder("PostError") { sqlSource ""; isError true }
    td.is_error |> should equal true

// ---------------------------------------------------------------------------
// QueryCEBuilder
// ---------------------------------------------------------------------------

[<Fact>]
let ``QueryCEBuilder produces correct QueryDefinition`` () =
    let qd =
        QueryCEBuilder("authors") {
            returnType "Author"
            returnsList true
            sqlSource "v_author"
        }

    qd.name |> should equal "authors"
    qd.return_type |> should equal "Author"
    qd.returns_list |> should equal true
    qd.sql_source |> should equal "v_author"

[<Fact>]
let ``QueryCEBuilder nullable via custom operation`` () =
    let qd = QueryCEBuilder("author") { returnType "Author"; sqlSource "v_author"; nullable true }
    qd.nullable |> should equal true

[<Fact>]
let ``QueryCEBuilder cacheTtlSeconds via custom operation`` () =
    let qd =
        QueryCEBuilder("authors") {
            returnType "Author"
            sqlSource "v_author"
            cacheTtlSeconds 60
        }

    qd.cache_ttl_seconds |> should equal (Some 60)

[<Fact>]
let ``QueryCEBuilder arg via custom operation`` () =
    let qd =
        QueryCEBuilder("authorById") {
            returnType "Author"
            sqlSource "v_author"
            arg "id" "ID" false
        }

    qd.arguments |> List.length |> should equal 1
    qd.arguments.[0].name |> should equal "id"
    qd.arguments.[0].type_ |> should equal "ID"
    qd.arguments.[0].nullable |> should equal false

// ---------------------------------------------------------------------------
// MutationCEBuilder
// ---------------------------------------------------------------------------

[<Fact>]
let ``MutationCEBuilder produces correct MutationDefinition`` () =
    let md =
        MutationCEBuilder("createAuthor") {
            returnType "Author"
            sqlSource "fn_create_author"
            operation "insert"
        }

    md.name |> should equal "createAuthor"
    md.return_type |> should equal "Author"
    md.sql_source |> should equal "fn_create_author"
    md.operation |> should equal "insert"

[<Fact>]
let ``MutationCEBuilder defaults operation to custom`` () =
    let md = MutationCEBuilder("doSomething") { returnType "Author"; sqlSource "fn_do" }
    md.operation |> should equal "custom"

[<Fact>]
let ``MutationCEBuilder arg via custom operation`` () =
    let md =
        MutationCEBuilder("updateAuthor") {
            returnType "Author"
            sqlSource "fn_update_author"
            operation "update"
            arg "id" "ID" false
            arg "name" "String" true
        }

    md.arguments |> List.length |> should equal 2

// ---------------------------------------------------------------------------
// FraiseQLBuilder (outer CE)
// ---------------------------------------------------------------------------

[<Fact>]
let ``fraiseql CE with type adds to types`` () =
    let td = TypeCEBuilder("Author") { sqlSource "v_author" }

    let schema =
        fraiseql {
            type' td
        }

    schema.types |> List.length |> should equal 1
    schema.types.[0].name |> should equal "Author"

[<Fact>]
let ``fraiseql CE with query adds to queries`` () =
    let qd = QueryCEBuilder("authors") { returnType "Author"; sqlSource "v_author"; returnsList true }

    let schema =
        fraiseql {
            query qd
        }

    schema.queries |> List.length |> should equal 1
    schema.queries.[0].name |> should equal "authors"

[<Fact>]
let ``fraiseql CE with mutation adds to mutations`` () =
    let md = MutationCEBuilder("createAuthor") { returnType "Author"; sqlSource "fn_create_author"; operation "insert" }

    let schema =
        fraiseql {
            mutation md
        }

    schema.mutations |> List.length |> should equal 1
    schema.mutations.[0].name |> should equal "createAuthor"

[<Fact>]
let ``fraiseql CE with multiple items`` () =
    let td = TypeCEBuilder("Author") { sqlSource "v_author" }
    let qd = QueryCEBuilder("authors") { returnType "Author"; sqlSource "v_author"; returnsList true }
    let md = MutationCEBuilder("createAuthor") { returnType "Author"; sqlSource "fn_create_author"; operation "insert" }

    let schema =
        fraiseql {
            type' td
            query qd
            mutation md
        }

    schema.types |> List.length |> should equal 1
    schema.queries |> List.length |> should equal 1
    schema.mutations |> List.length |> should equal 1

[<Fact>]
let ``fraiseql CE does not touch SchemaRegistry`` () =
    SchemaRegistry.reset ()
    let td = TypeCEBuilder("DSLOnly") { sqlSource "v_dsl" }

    let _schema =
        fraiseql {
            type' td
        }

    SchemaRegistry.getAllTypes () |> List.length |> should equal 0

// ---------------------------------------------------------------------------
// DSL JSON round-trip
// ---------------------------------------------------------------------------

[<Fact>]
let ``fraiseql CE exported to JSON has correct structure`` () =
    let td =
        TypeCEBuilder("Author") {
            sqlSource "v_author"
            field (FieldBuilder("id", "ID") { nullable false })
            field (FieldBuilder("name", "String") { nullable false })
        }

    let qd =
        QueryCEBuilder("authors") {
            returnType "Author"
            returnsList true
            sqlSource "v_author"
        }

    let schema =
        fraiseql {
            type' td
            query qd
        }

    let json = SchemaExporter.exportSchema schema
    let root = parseJson json
    root.GetProperty("version").GetString() |> should equal "2.0.0"
    root.GetProperty("types").GetArrayLength() |> should equal 1
    root.GetProperty("queries").GetArrayLength() |> should equal 1
    root.GetProperty("types").[0].GetProperty("sql_source").GetString() |> should equal "v_author"
    root.GetProperty("queries").[0].GetProperty("returns_list").GetBoolean() |> should equal true
