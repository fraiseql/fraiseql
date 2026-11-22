/// A `System.DateTime` carries its zone in a runtime `Kind`, not in its type, so the type
/// alone cannot tell a PostgreSQL `timestamp` from a `timestamptz`: it stays `DateTime`. A
/// date and time with no offset is declared with the attribute's type string,
/// `[<GraphQLField(Type = "LocalDateTime")>]`, which wins over inference.
module FraiseQL.Tests.LocalDateTimeTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

[<GraphQLType(Name = "LocalDateTimeShift", SqlSource = "v_shift")>]
type LocalDateTimeShift() =
    [<GraphQLField(Type = "LocalDateTime")>]
    member val StartsAt = DateTime.MinValue with get, set

    [<GraphQLField(Type = "LocalDateTime")>]
    member val EndsAt: DateTime option = None with get, set

    [<GraphQLField(Type = "[LocalDateTime!]")>]
    member val BreaksAt: DateTime list = [] with get, set

    [<GraphQLField>]
    member val LoggedAt = DateTime.MinValue with get, set

    [<GraphQLField>]
    member val ClockedAt = DateTimeOffset.MinValue with get, set

[<Fact>]
let ``the type string declares LocalDateTime on a DateTime property; an undeclared one stays DateTime`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<LocalDateTimeShift>
    let fields = (SchemaRegistry.getTypeDefinition "LocalDateTimeShift").Value.fields
    let field name = fields |> List.find (fun f -> f.name = name)
    (field "startsAt").type_ |> should equal "LocalDateTime"
    (field "startsAt").nullable |> should equal false
    (field "endsAt").type_ |> should equal "LocalDateTime"
    (field "endsAt").nullable |> should equal true
    (field "breaksAt").type_ |> should equal "[LocalDateTime!]"
    (field "loggedAt").type_ |> should equal "DateTime"
    (field "clockedAt").type_ |> should equal "DateTime"
    SchemaRegistry.reset ()
