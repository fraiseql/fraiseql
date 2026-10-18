module FraiseQL.Tests.EnumValueTests

open System.Text.Json
open System.Text.Json.Serialization
open Xunit
open FsUnit.Xunit
open FraiseQL

// #1611: a member is stored as the value F# gives it: a .NET enum's integer (or its name when
// the enum is serialized as strings), a fieldless union case's name.

type Priority =
    | Low = 1
    | High = 3

[<JsonConverter(typeof<JsonStringEnumConverter>)>]
type Flagged =
    | Off = 0
    | On = 1

type Status =
    | Pending
    | Shipped

type Shape =
    | Point
    | Circle of radius: float

let private values (name: string) =
    use root = JsonDocument.Parse(SchemaExporter.export ())

    root.RootElement.GetProperty("enums").EnumerateArray()
    |> Seq.find (fun e -> e.GetProperty("name").GetString() = name)
    |> fun e -> JsonSerializer.Serialize(e.GetProperty("values"))

[<Fact>]
let ``a .NET enum is stored as its integers`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.registerEnum typeof<Priority> (Some "The priority of a task")

    values "Priority"
    |> should equal """[{"name":"Low","value":1},{"name":"High","value":3}]"""

[<Fact>]
let ``an enum serialized as strings is stored as its names`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.registerEnum typeof<Flagged> None

    values "Flagged" |> should equal """[{"name":"Off"},{"name":"On"}]"""

[<Fact>]
let ``a fieldless union is stored as its case names`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.registerEnum typeof<Status> None

    values "Status" |> should equal """[{"name":"Pending"},{"name":"Shipped"}]"""

[<Fact>]
let ``a union case carrying fields is refused, named`` () =
    SchemaRegistry.reset ()

    (fun () -> SchemaRegistry.registerEnum typeof<Shape> None)
    |> should (throwWithMessage "Shape.Circle carries fields; an enum member carries none (a fieldless union case or a .NET enum member)") typeof<System.ArgumentException>
