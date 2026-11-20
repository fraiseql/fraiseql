/// A field's required scope, declared as `Scope` or as a one-item `Scopes` list, reaches
/// the exported `requires_scope`; a declaration the compiled schema cannot hold is refused.
module FraiseQL.Tests.FieldScopeTests

open System.Text.Json
open Xunit
open FsUnit.Xunit
open FraiseQL

[<GraphQLType(Name = "Payroll", SqlSource = "v_payroll")>]
type Payroll() =
    [<GraphQLField(Type = "ID", Nullable = false)>]
    member val Id = "" with get, set

    [<GraphQLField(Type = "Float", Nullable = true, Scopes = [| "read:Payroll.salary" |])>]
    member val Salary = 0.0 with get, set

    [<GraphQLField(Type = "String", Nullable = true, Scope = "read:Payroll.iban")>]
    member val Iban = "" with get, set

[<GraphQLType(Name = "TwoScopes", SqlSource = "v_two_scopes")>]
type TwoScopes() =
    [<GraphQLField(Type = "String", Nullable = true, Scopes = [| "a"; "b" |])>]
    member val Secret = "" with get, set

[<GraphQLType(Name = "BothForms", SqlSource = "v_both_forms")>]
type BothForms() =
    [<GraphQLField(Type = "String", Nullable = true, Scope = "a", Scopes = [| "b" |])>]
    member val Secret = "" with get, set

let private field (fieldName: string) : JsonElement =
    use document = JsonDocument.Parse(SchemaExporter.export ())

    document.RootElement.GetProperty("types").EnumerateArray()
    |> Seq.filter (fun t -> t.GetProperty("name").GetString() = "Payroll")
    |> Seq.collect (fun t -> t.GetProperty("fields").EnumerateArray())
    |> Seq.filter (fun f -> f.GetProperty("name").GetString() = fieldName)
    |> Seq.map (fun f -> f.Clone())
    |> Seq.exactlyOne

[<Fact>]
let ``a one-item Scopes list is exported as the required scope`` () =
    SchemaRegistry.reset ()
    SchemaRegistry.register typeof<Payroll>

    (field "salary").GetProperty("requires_scope").GetString()
    |> should equal "read:Payroll.salary"

    (field "iban").GetProperty("requires_scope").GetString()
    |> should equal "read:Payroll.iban"

    (field "id").TryGetProperty("requires_scope") |> fst |> should equal false

[<Fact>]
let ``a longer Scopes list is refused`` () =
    SchemaRegistry.reset ()

    let thrown =
        Assert.Throws<System.ArgumentException>(fun () -> SchemaRegistry.register typeof<TwoScopes>)

    thrown.Message |> should haveSubstring "'secret'"
    thrown.Message |> should haveSubstring "2 scopes"

[<Fact>]
let ``Scope and Scopes together are refused`` () =
    SchemaRegistry.reset ()

    let thrown =
        Assert.Throws<System.ArgumentException>(fun () -> SchemaRegistry.register typeof<BothForms>)

    thrown.Message |> should haveSubstring "both Scope and Scopes"
