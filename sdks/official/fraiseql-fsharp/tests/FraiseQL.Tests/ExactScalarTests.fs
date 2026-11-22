/// A `decimal` is the engine's exact `Decimal`, and a `Guid` its `UUID`; a property named
/// `Id` of it is `ID`. As `Float` a decimal loses digits, and as `ID` a Guid that is not the
/// type's `id`, a reference, loses its type.
module FraiseQL.Tests.ExactScalarTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

[<GraphQLType(Name = "ExactQuote", SqlSource = "v_quote")>]
type ExactQuote() =
    [<GraphQLField>]
    member val Id = Guid.Empty with get, set

    [<GraphQLField>]
    member val Amount = 0m with get, set

    [<GraphQLField>]
    member val Deltas: decimal list = [] with get, set

    [<GraphQLField>]
    member val IssuerId: Guid option = None with get, set

[<Fact>]
let ``decimal is Decimal and Guid is UUID in both mappers`` () =
    TypeMapper.toGraphQLType typeof<decimal> |> should equal "Decimal"
    TypeMapper.toGraphQLType typeof<decimal option> |> should equal "Decimal"
    TypeMapper.toGraphQLType typeof<decimal list> |> should equal "[Decimal!]"
    TypeMapper.toGraphQLType typeof<Guid> |> should equal "UUID"
    TypeMapper.toGraphQLScalar typeof<decimal> |> should equal (GqlCustom "Decimal")
    TypeMapper.toGraphQLScalar typeof<Guid> |> should equal (GqlCustom "UUID")
    TypeMapper.toGraphQLType typeof<float> |> should equal "Float"

[<Fact>]
let ``a type's Guid id registers as ID and any other Guid as UUID`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<ExactQuote>
    let fields = (SchemaRegistry.getTypeDefinition "ExactQuote").Value.fields
    let field name = fields |> List.find (fun f -> f.name = name)
    (field "id").type_ |> should equal "ID"
    (field "amount").type_ |> should equal "Decimal"
    (field "deltas").type_ |> should equal "[Decimal!]"
    (field "issuerId").type_ |> should equal "UUID"
    (field "issuerId").nullable |> should equal true
    SchemaRegistry.reset ()

[<GraphQLType(Name = "ExactUntyped", SqlSource = "v_untyped")>]
type ExactUntyped() =
    [<GraphQLField>]
    member val Anything = obj () with get, set

[<GraphQLType(Name = "ExactDeclared", SqlSource = "v_declared")>]
type ExactDeclared() =
    [<GraphQLField(Type = "Json")>]
    member val Payload = obj () with get, set

/// An untyped `obj` had been published as `String`, the case where a silent default is
/// most wrong: it is refused, in both mappers, naming the fix.
[<Fact>]
let ``obj is refused, pointing to a declared type`` () =
    for refused in
        [ Assert.Throws<NotSupportedException>(fun () -> TypeMapper.toGraphQLType typeof<obj> |> ignore)
          Assert.Throws<NotSupportedException>(fun () -> TypeMapper.toGraphQLScalar typeof<obj> |> ignore) ] do
        refused.Message |> should haveSubstring "Object"
        refused.Message |> should haveSubstring "[<GraphQLField(Type = \"...\")>]"

[<Fact>]
let ``an obj member is refused at registration, naming it, unless it names a type`` () =
    SchemaRegistry.reset ()
    let refused =
        Assert.Throws<NotSupportedException>(fun () -> SchemaRegistry.register typeof<ExactUntyped>)
    refused.Message |> should haveSubstring "ExactUntyped.Anything"
    SchemaRegistry.register typeof<ExactDeclared>
    let field = (SchemaRegistry.getTypeDefinition "ExactDeclared").Value.fields |> List.exactlyOne
    field.type_ |> should equal "Json"
    SchemaRegistry.reset ()
