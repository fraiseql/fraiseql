module FraiseQL.Tests.AttributeTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

// ---------------------------------------------------------------------------
// Fixture types decorated with attributes
// ---------------------------------------------------------------------------

[<GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")>]
type AuthorClass() =
    [<GraphQLField(Nullable = false, Description = "Author unique identifier")>]
    member val Id: Guid = Guid.Empty with get, set

    [<GraphQLField(Nullable = false)>]
    member val Name: string = "" with get, set

    [<GraphQLField(Nullable = true)>]
    member val Bio: string = null with get, set

[<GraphQLType>]
type MinimalClass() =
    [<GraphQLField>]
    member val Value: int = 0 with get, set

[<GraphQLType(IsInput = true)>]
type InputClass() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val UserId: string = "" with get, set

[<GraphQLType(Relay = true, SqlSource = "v_posts")>]
type RelayClass() =
    [<GraphQLField(Nullable = false)>]
    member val Id: Guid = Guid.Empty with get, set

[<GraphQLType(IsError = true)>]
type ErrorClass() =
    [<GraphQLField(Nullable = false)>]
    member val Code: string = "" with get, set

[<GraphQLType(SqlSource = "v_tagged")>]
type TaggedClass() =
    [<GraphQLField(Scope = "read:articles")>]
    member val Title: string = "" with get, set

    [<GraphQLField(Scopes = [| "admin"; "moderator" |])>]
    member val Secret: string = "" with get, set

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

[<Fact>]
let ``applyGraphQLTypeAttribute reads Name correctly`` () =
    let attr = typeof<AuthorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.Name |> should equal "Author"

[<Fact>]
let ``applyGraphQLTypeAttribute reads SqlSource correctly`` () =
    let attr = typeof<AuthorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.SqlSource |> should equal "v_author"

[<Fact>]
let ``applyGraphQLTypeAttribute reads Description correctly`` () =
    let attr = typeof<AuthorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.Description |> should equal "A blog author"

[<Fact>]
let ``applyGraphQLTypeAttribute IsInput defaults to false`` () =
    let attr = typeof<AuthorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.IsInput |> should equal false

[<Fact>]
let ``applyGraphQLTypeAttribute Relay defaults to false`` () =
    let attr = typeof<AuthorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.Relay |> should equal false

[<Fact>]
let ``applyGraphQLTypeAttribute IsError defaults to false`` () =
    let attr = typeof<AuthorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.IsError |> should equal false

[<Fact>]
let ``applyGraphQLTypeAttribute IsInput set to true`` () =
    let attr = typeof<InputClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.IsInput |> should equal true

[<Fact>]
let ``applyGraphQLTypeAttribute Relay set to true`` () =
    let attr = typeof<RelayClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.Relay |> should equal true

[<Fact>]
let ``applyGraphQLTypeAttribute IsError set to true`` () =
    let attr = typeof<ErrorClass>.GetCustomAttributes(typeof<GraphQLTypeAttribute>, false).[0] :?> GraphQLTypeAttribute
    attr.IsError |> should equal true

[<Fact>]
let ``applyGraphQLFieldAttribute reads Type correctly via explicit override`` () =
    let prop = typeof<InputClass>.GetProperty("UserId")
    let attr = prop.GetCustomAttributes(typeof<GraphQLFieldAttribute>, false).[0] :?> GraphQLFieldAttribute
    attr.Type |> should equal "ID"

[<Fact>]
let ``applyGraphQLFieldAttribute Nullable defaults to true`` () =
    let prop = typeof<MinimalClass>.GetProperty("Value")
    let attr = prop.GetCustomAttributes(typeof<GraphQLFieldAttribute>, false).[0] :?> GraphQLFieldAttribute
    attr.Nullable |> should equal true

[<Fact>]
let ``applyGraphQLFieldAttribute reads Nullable false`` () =
    let prop = typeof<AuthorClass>.GetProperty("Id")
    let attr = prop.GetCustomAttributes(typeof<GraphQLFieldAttribute>, false).[0] :?> GraphQLFieldAttribute
    attr.Nullable |> should equal false

[<Fact>]
let ``applyGraphQLFieldAttribute reads Description correctly`` () =
    let prop = typeof<AuthorClass>.GetProperty("Id")
    let attr = prop.GetCustomAttributes(typeof<GraphQLFieldAttribute>, false).[0] :?> GraphQLFieldAttribute
    attr.Description |> should equal "Author unique identifier"

[<Fact>]
let ``applyGraphQLFieldAttribute reads Scopes array`` () =
    let prop = typeof<TaggedClass>.GetProperty("Secret")
    let attr = prop.GetCustomAttributes(typeof<GraphQLFieldAttribute>, false).[0] :?> GraphQLFieldAttribute
    attr.Scopes |> should equal [| "admin"; "moderator" |]

[<Fact>]
let ``applyGraphQLFieldAttribute reads Scope string`` () =
    let prop = typeof<TaggedClass>.GetProperty("Title")
    let attr = prop.GetCustomAttributes(typeof<GraphQLFieldAttribute>, false).[0] :?> GraphQLFieldAttribute
    attr.Scope |> should equal "read:articles"

[<Fact>]
let ``graphQLTypeAttribute is sealed`` () =
    typeof<GraphQLTypeAttribute>.IsSealed |> should equal true

[<Fact>]
let ``graphQLFieldAttribute is sealed`` () =
    typeof<GraphQLFieldAttribute>.IsSealed |> should equal true

[<Fact>]
let ``graphQLTypeAttribute does not allow multiple`` () =
    let usageAttr =
        typeof<GraphQLTypeAttribute>.GetCustomAttributes(typeof<AttributeUsageAttribute>, false).[0]
        :?> AttributeUsageAttribute
    usageAttr.AllowMultiple |> should equal false

[<Fact>]
let ``graphQLFieldAttribute does not allow multiple`` () =
    let usageAttr =
        typeof<GraphQLFieldAttribute>.GetCustomAttributes(typeof<AttributeUsageAttribute>, false).[0]
        :?> AttributeUsageAttribute
    usageAttr.AllowMultiple |> should equal false
