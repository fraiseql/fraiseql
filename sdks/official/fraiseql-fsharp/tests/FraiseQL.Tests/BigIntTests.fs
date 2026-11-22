/// An integer maps to the narrowest scalar holding its range: `Int` is 32 bits and `BigInt`
/// a signed 64, so `int64` and `uint32` are `BigInt`, and no integer scalar holds a `uint64`.
module FraiseQL.Tests.BigIntTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

[<GraphQLType(Name = "BigIntLedger", SqlSource = "v_ledger")>]
type BigIntLedger() =
    [<GraphQLField>]
    member val Amount = 0L with get, set

    [<GraphQLField>]
    member val Balance: int64 option = None with get, set

    [<GraphQLField>]
    member val TransferIds: int64 list = [] with get, set

    [<GraphQLField(Type = "Decimal")>]
    member val Total = 0UL with get, set

[<GraphQLType(Name = "UnsignedLedger", SqlSource = "v_ledger")>]
type UnsignedLedger() =
    [<GraphQLField>]
    member val Total = 0UL with get, set

[<Fact>]
let ``64-bit and unsigned 32-bit integers map to BigInt`` () =
    TypeMapper.toGraphQLType typeof<int64> |> should equal "BigInt"
    TypeMapper.toGraphQLType typeof<uint32> |> should equal "BigInt"
    TypeMapper.toGraphQLType typeof<int64 option> |> should equal "BigInt"
    TypeMapper.toGraphQLType typeof<Nullable<int64>> |> should equal "BigInt"
    TypeMapper.toGraphQLType typeof<int64 list> |> should equal "[BigInt!]"
    TypeMapper.toGraphQLType typeof<int64 option list> |> should equal "[BigInt]"
    TypeMapper.toGraphQLType typeof<int32> |> should equal "Int"
    TypeMapper.toGraphQLScalar typeof<int64> |> should equal (GqlCustom "BigInt")
    TypeMapper.toGraphQLScalar typeof<uint32> |> should equal (GqlCustom "BigInt")

[<Fact>]
let ``an unsigned 64-bit integer is refused, pointing to Decimal`` () =
    for t in [ typeof<uint64>; typeof<uint64 option>; typeof<uint64 list> ] do
        let refused = Assert.Throws<NotSupportedException>(fun () -> TypeMapper.toGraphQLType t |> ignore)
        refused.Message |> should haveSubstring "UInt64"
        refused.Message |> should haveSubstring "Decimal"
    Assert.Throws<NotSupportedException>(fun () -> TypeMapper.toGraphQLScalar typeof<uint64> |> ignore)
    |> ignore

[<Fact>]
let ``a type's int64 fields register as BigInt, and a uint64 one only when named`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<BigIntLedger>
    let fields = (SchemaRegistry.getTypeDefinition "BigIntLedger").Value.fields
    let field name = fields |> List.find (fun f -> f.name = name)
    (field "amount").type_ |> should equal "BigInt"
    (field "amount").nullable |> should equal false
    (field "balance").type_ |> should equal "BigInt"
    (field "balance").nullable |> should equal true
    (field "transferIds").type_ |> should equal "[BigInt!]"
    (field "total").type_ |> should equal "Decimal"

    let refused =
        Assert.Throws<NotSupportedException>(fun () -> SchemaRegistry.register typeof<UnsignedLedger>)
    refused.Message |> should haveSubstring "UnsignedLedger.Total"
    SchemaRegistry.reset ()
