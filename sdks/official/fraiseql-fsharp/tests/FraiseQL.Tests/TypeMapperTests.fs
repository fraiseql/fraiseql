module FraiseQL.Tests.TypeMapperTests

open System
open Xunit
open FsUnit.Xunit
open FraiseQL

[<Fact>]
let ``int maps to Int`` () =
    TypeMapper.toGraphQLType typeof<int> |> should equal "Int"

[<Fact>]
let ``int64 maps to Int`` () =
    TypeMapper.toGraphQLType typeof<int64> |> should equal "Int"

[<Fact>]
let ``int16 maps to Int`` () =
    TypeMapper.toGraphQLType typeof<int16> |> should equal "Int"

[<Fact>]
let ``float maps to Float`` () =
    TypeMapper.toGraphQLType typeof<float> |> should equal "Float"

[<Fact>]
let ``float32 maps to Float`` () =
    TypeMapper.toGraphQLType typeof<float32> |> should equal "Float"

[<Fact>]
let ``decimal maps to Float`` () =
    TypeMapper.toGraphQLType typeof<decimal> |> should equal "Float"

[<Fact>]
let ``bool maps to Boolean`` () =
    TypeMapper.toGraphQLType typeof<bool> |> should equal "Boolean"

[<Fact>]
let ``string maps to String`` () =
    TypeMapper.toGraphQLType typeof<string> |> should equal "String"

[<Fact>]
let ``Guid maps to ID`` () =
    TypeMapper.toGraphQLType typeof<Guid> |> should equal "ID"

[<Fact>]
let ``DateTime maps to DateTime`` () =
    TypeMapper.toGraphQLType typeof<DateTime> |> should equal "DateTime"

[<Fact>]
let ``DateTimeOffset maps to DateTime`` () =
    TypeMapper.toGraphQLType typeof<DateTimeOffset> |> should equal "DateTime"

[<Fact>]
let ``string option maps to String and is nullable`` () =
    TypeMapper.toGraphQLType typeof<string option> |> should equal "String"
    TypeMapper.isNullable typeof<string option> |> should equal true

[<Fact>]
let ``int option maps to Int and is nullable`` () =
    TypeMapper.toGraphQLType typeof<int option> |> should equal "Int"
    TypeMapper.isNullable typeof<int option> |> should equal true

[<Fact>]
let ``Guid list maps to [ID]`` () =
    TypeMapper.toGraphQLType typeof<Guid list> |> should equal "[ID]"

[<Fact>]
let ``int array maps to [Int]`` () =
    TypeMapper.toGraphQLType typeof<int array> |> should equal "[Int]"

[<Fact>]
let ``string list is not nullable`` () =
    TypeMapper.isNullable typeof<string list> |> should equal false

[<Fact>]
let ``Nullable int is nullable`` () =
    TypeMapper.isNullable typeof<Nullable<int>> |> should equal true

[<Fact>]
let ``Nullable int maps to Int`` () =
    TypeMapper.toGraphQLType typeof<Nullable<int>> |> should equal "Int"

[<Fact>]
let ``unknown record type maps to its type name`` () =
    // A record type not in the scalar map returns its .NET name
    TypeMapper.toGraphQLType typeof<FieldDefinition> |> should equal "FieldDefinition"

[<Fact>]
let ``toGraphQLTypeWithNullability returns pair for option type`` () =
    let gqlType, isNull = TypeMapper.toGraphQLTypeWithNullability typeof<Guid option>
    gqlType |> should equal "ID"
    isNull |> should equal true

[<Fact>]
let ``toGraphQLScalar returns GqlInt for int`` () =
    TypeMapper.toGraphQLScalar typeof<int> |> should equal GqlInt

[<Fact>]
let ``toGraphQLScalar returns GqlId for Guid`` () =
    TypeMapper.toGraphQLScalar typeof<Guid> |> should equal GqlId

[<Fact>]
let ``toGraphQLScalar returns GqlString for string`` () =
    TypeMapper.toGraphQLScalar typeof<string> |> should equal GqlString

[<Fact>]
let ``toSnakeCase converts PascalCase`` () =
    TypeMapper.toSnakeCase "SqlSource" |> should equal "sql_source"

[<Fact>]
let ``toSnakeCase converts camelCase`` () =
    TypeMapper.toSnakeCase "sqlSource" |> should equal "sql_source"

[<Fact>]
let ``toSnakeCase leaves already snake_case unchanged`` () =
    TypeMapper.toSnakeCase "sql_source" |> should equal "sql_source"

[<Fact>]
let ``toSnakeCase handles empty string`` () =
    TypeMapper.toSnakeCase "" |> should equal ""
