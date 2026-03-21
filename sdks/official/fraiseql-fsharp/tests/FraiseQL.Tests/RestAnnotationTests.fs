module FraiseQL.Tests.RestAnnotationTests

open Xunit
open FraiseQL

[<Fact>]
let ``Query with restPath emits rest fields in definition`` () =
    let def =
        QueryBuilder.query "users"
        |> QueryBuilder.returnType "User"
        |> QueryBuilder.returnsList true
        |> QueryBuilder.sqlSource "v_user"
        |> QueryBuilder.restPath "/api/users"
        |> QueryBuilder.restMethod "GET"
        |> QueryBuilder.toDefinition

    Assert.Equal(Some "/api/users", def.rest_path)
    Assert.Equal(Some "GET", def.rest_method)

[<Fact>]
let ``Query without restPath has None`` () =
    let def =
        QueryBuilder.query "users"
        |> QueryBuilder.returnType "User"
        |> QueryBuilder.returnsList true
        |> QueryBuilder.sqlSource "v_user"
        |> QueryBuilder.toDefinition

    Assert.Equal(None, def.rest_path)
    Assert.Equal(None, def.rest_method)

[<Fact>]
let ``Mutation with restPath emits rest fields`` () =
    let def =
        MutationBuilder.mutation "createUser"
        |> MutationBuilder.returnType "User"
        |> MutationBuilder.sqlSource "fn_create_user"
        |> MutationBuilder.operation "insert"
        |> MutationBuilder.restPath "/api/users"
        |> MutationBuilder.restMethod "POST"
        |> MutationBuilder.toDefinition

    Assert.Equal(Some "/api/users", def.rest_path)
    Assert.Equal(Some "POST", def.rest_method)

[<Fact>]
let ``Mutation without restPath has None`` () =
    let def =
        MutationBuilder.mutation "createUser"
        |> MutationBuilder.returnType "User"
        |> MutationBuilder.sqlSource "fn_create_user"
        |> MutationBuilder.operation "insert"
        |> MutationBuilder.toDefinition

    Assert.Equal(None, def.rest_path)
    Assert.Equal(None, def.rest_method)
