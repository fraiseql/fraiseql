module FraiseQL.Tests.SchemaRegistryTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

// ---------------------------------------------------------------------------
// Fixture types
// ---------------------------------------------------------------------------

[<GraphQLType(Name = "Post", SqlSource = "v_post", Description = "A blog post")>]
type PostEntity() =
    [<GraphQLField(Nullable = false)>]
    member val Id: Guid = Guid.Empty with get, set

    [<GraphQLField(Nullable = false)>]
    member val Title: string = "" with get, set

    [<GraphQLField(Nullable = true)>]
    member val Body: string = null with get, set

[<GraphQLType(Name = "Comment", SqlSource = "v_comment")>]
type CommentEntity() =
    [<GraphQLField(Nullable = false)>]
    member val Id: Guid = Guid.Empty with get, set

    [<GraphQLField(Type = "String", Nullable = false)>]
    member val Text: string = "" with get, set

[<GraphQLType(Name = "Tag", SqlSource = "v_tag", IsInput = true)>]
type TagEntity() =
    [<GraphQLField(Nullable = false)>]
    member val Name: string = "" with get, set

[<GraphQLType(Name = "RelayPost", SqlSource = "v_relay_post", Relay = true)>]
type RelayPostEntity() =
    [<GraphQLField(Nullable = false)>]
    member val Id: Guid = Guid.Empty with get, set

type NoAttributeClass() =
    member val Id: int = 0 with get, set

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let private cleanup () = SchemaRegistry.reset ()

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``register extracts name from attribute`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    td |> Option.isSome |> should equal true
    td.Value.name |> should equal "Post"

[<Fact>]
let ``register extracts sql_source from attribute`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    td.Value.sql_source |> should equal "v_post"

[<Fact>]
let ``register extracts description from attribute`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    td.Value.description |> should equal (Some "A blog post")

[<Fact>]
let ``register sets is_input correctly`` () =
    cleanup ()
    SchemaRegistry.register typeof<TagEntity>
    let td = SchemaRegistry.getTypeDefinition "Tag"
    td.Value.is_input |> should equal true

[<Fact>]
let ``register sets relay correctly`` () =
    cleanup ()
    SchemaRegistry.register typeof<RelayPostEntity>
    let td = SchemaRegistry.getTypeDefinition "RelayPost"
    td.Value.relay |> should equal true

[<Fact>]
let ``register reflects fields with GraphQLField attribute`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    td.Value.fields |> List.length |> should equal 3

[<Fact>]
let ``register infers field type from .NET type`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    let idField = td.Value.fields |> List.find (fun f -> f.name = "id")
    idField.type_ |> should equal "ID"

[<Fact>]
let ``register respects explicit Type override on field`` () =
    cleanup ()
    SchemaRegistry.register typeof<CommentEntity>
    let td = SchemaRegistry.getTypeDefinition "Comment"
    let textField = td.Value.fields |> List.find (fun f -> f.name = "text")
    textField.type_ |> should equal "String"

[<Fact>]
let ``register respects nullable false on field`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    let idField = td.Value.fields |> List.find (fun f -> f.name = "id")
    idField.nullable |> should equal false

[<Fact>]
let ``register respects nullable true on field`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    let td = SchemaRegistry.getTypeDefinition "Post"
    let bodyField = td.Value.fields |> List.find (fun f -> f.name = "body")
    bodyField.nullable |> should equal true

[<Fact>]
let ``register multiple types`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    SchemaRegistry.register typeof<CommentEntity>
    SchemaRegistry.getAllTypes () |> List.length |> should equal 2

[<Fact>]
let ``reset clears all types`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    SchemaRegistry.register typeof<CommentEntity>
    SchemaRegistry.reset ()
    SchemaRegistry.getAllTypes () |> List.length |> should equal 0

[<Fact>]
let ``reset clears all queries`` () =
    cleanup ()

    let q: QueryDefinition =
        {
            name = "posts"
            return_type = "Post"
            returns_list = true
            nullable = false
            sql_source = "v_post"
            arguments = []
            cache_ttl_seconds = None
            description = None
        }

    SchemaRegistry.registerQuery q
    SchemaRegistry.reset ()
    SchemaRegistry.getAllQueries () |> List.length |> should equal 0

[<Fact>]
let ``reset clears all mutations`` () =
    cleanup ()

    let m: MutationDefinition =
        {
            name = "createPost"
            return_type = "Post"
            sql_source = "fn_create_post"
            operation = "insert"
            arguments = []
            description = None
        }

    SchemaRegistry.registerMutation m
    SchemaRegistry.reset ()
    SchemaRegistry.getAllMutations () |> List.length |> should equal 0

[<Fact>]
let ``register duplicate name overwrites previous registration`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>
    SchemaRegistry.register typeof<PostEntity>
    SchemaRegistry.getAllTypes () |> List.length |> should equal 1

[<Fact>]
let ``register raises ArgumentException for type without attribute`` () =
    cleanup ()

    (fun () -> SchemaRegistry.register typeof<NoAttributeClass>)
    |> should throw typeof<ArgumentException>

[<Fact>]
let ``getTypeDefinition returns None for unregistered type`` () =
    cleanup ()
    SchemaRegistry.getTypeDefinition "Unknown" |> should equal None

[<Fact>]
let ``registerQuery adds to getAllQueries`` () =
    cleanup ()

    let q: QueryDefinition =
        {
            name = "allPosts"
            return_type = "Post"
            returns_list = true
            nullable = false
            sql_source = "v_post"
            arguments = []
            cache_ttl_seconds = None
            description = None
        }

    SchemaRegistry.registerQuery q
    let qs = SchemaRegistry.getAllQueries ()
    qs |> List.length |> should equal 1
    qs.[0].name |> should equal "allPosts"

[<Fact>]
let ``registerMutation adds to getAllMutations`` () =
    cleanup ()

    let m: MutationDefinition =
        {
            name = "deletePost"
            return_type = "Post"
            sql_source = "fn_delete_post"
            operation = "delete"
            arguments = []
            description = None
        }

    SchemaRegistry.registerMutation m
    let ms = SchemaRegistry.getAllMutations ()
    ms |> List.length |> should equal 1
    ms.[0].name |> should equal "deletePost"

[<Fact>]
let ``toIntermediateSchema includes all registered items`` () =
    cleanup ()
    SchemaRegistry.register typeof<PostEntity>

    let q: QueryDefinition =
        {
            name = "posts"
            return_type = "Post"
            returns_list = true
            nullable = false
            sql_source = "v_post"
            arguments = []
            cache_ttl_seconds = None
            description = None
        }

    SchemaRegistry.registerQuery q
    let schema = SchemaRegistry.toIntermediateSchema ()
    schema.version |> should equal "2.0.0"
    schema.types |> List.length |> should equal 1
    schema.queries |> List.length |> should equal 1
    schema.mutations |> List.length |> should equal 0

[<Fact>]
let ``QueryBuilder pipeline produces correct definition`` () =
    cleanup ()

    QueryBuilder.query "authors"
    |> QueryBuilder.returnType "Author"
    |> QueryBuilder.returnsList true
    |> QueryBuilder.sqlSource "v_author"
    |> QueryBuilder.register

    let qs = SchemaRegistry.getAllQueries ()
    qs |> List.length |> should equal 1
    qs.[0].name |> should equal "authors"
    qs.[0].return_type |> should equal "Author"
    qs.[0].returns_list |> should equal true
    qs.[0].sql_source |> should equal "v_author"

[<Fact>]
let ``MutationBuilder pipeline produces correct definition`` () =
    cleanup ()

    MutationBuilder.mutation "createPost"
    |> MutationBuilder.returnType "Post"
    |> MutationBuilder.sqlSource "fn_create_post"
    |> MutationBuilder.operation "insert"
    |> MutationBuilder.register

    let ms = SchemaRegistry.getAllMutations ()
    ms |> List.length |> should equal 1
    ms.[0].name |> should equal "createPost"
    ms.[0].return_type |> should equal "Post"
    ms.[0].operation |> should equal "insert"

[<Fact>]
let ``QueryBuilder toDefinition raises when returnType is missing`` () =
    (fun () ->
        QueryBuilder.query "bad"
        |> QueryBuilder.sqlSource "v_x"
        |> QueryBuilder.toDefinition
        |> ignore)
    |> should throw typeof<InvalidOperationException>

[<Fact>]
let ``QueryBuilder toDefinition raises when sqlSource is missing`` () =
    (fun () ->
        QueryBuilder.query "bad"
        |> QueryBuilder.returnType "X"
        |> QueryBuilder.toDefinition
        |> ignore)
    |> should throw typeof<InvalidOperationException>

[<Fact>]
let ``MutationBuilder toDefinition raises when returnType is missing`` () =
    (fun () ->
        MutationBuilder.mutation "bad"
        |> MutationBuilder.sqlSource "fn_x"
        |> MutationBuilder.toDefinition
        |> ignore)
    |> should throw typeof<InvalidOperationException>

[<Fact>]
let ``MutationBuilder toDefinition raises when sqlSource is missing`` () =
    (fun () ->
        MutationBuilder.mutation "bad"
        |> MutationBuilder.returnType "X"
        |> MutationBuilder.toDefinition
        |> ignore)
    |> should throw typeof<InvalidOperationException>

[<Fact>]
let ``QueryBuilder withArgument adds argument`` () =
    cleanup ()

    QueryBuilder.query "postById"
    |> QueryBuilder.returnType "Post"
    |> QueryBuilder.sqlSource "v_post"
    |> QueryBuilder.withArgument "id" "ID" false
    |> QueryBuilder.register

    let qs = SchemaRegistry.getAllQueries ()
    qs.[0].arguments |> List.length |> should equal 1
    qs.[0].arguments.[0].name |> should equal "id"
    qs.[0].arguments.[0].type_ |> should equal "ID"

[<Fact>]
let ``concurrent register is thread-safe`` () =
    cleanup ()

    let tasks =
        [| for _ in 1..10 do
               System.Threading.Tasks.Task.Run(fun () ->
                   SchemaRegistry.register typeof<PostEntity>) |]

    System.Threading.Tasks.Task.WaitAll(tasks)
    SchemaRegistry.getAllTypes () |> List.length |> should equal 1
