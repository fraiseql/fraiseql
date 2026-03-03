namespace FraiseQL

/// Maps .NET <see cref="System.Type"/> values to GraphQL type strings and scalars.
/// Used by <see cref="SchemaRegistry"/> when inferring field types from reflection.
module TypeMapper =

    /// Unwraps F# <c>option&lt;'T&gt;</c>, returning the inner type if present.
    let private unwrapOption (t: System.Type) =
        if t.IsGenericType && t.GetGenericTypeDefinition() = typedefof<option<_>> then
            Some(t.GetGenericArguments().[0])
        else
            None

    /// Unwraps F# <c>list&lt;'T&gt;</c> or <c>'T array</c>, returning the element type if present.
    let private unwrapList (t: System.Type) =
        if
            t.IsGenericType
            && (t.GetGenericTypeDefinition() = typedefof<list<_>>
                || t.GetGenericTypeDefinition() = typedefof<System.Collections.Generic.IEnumerable<_>>)
        then
            Some(t.GetGenericArguments().[0])
        elif t.IsArray then
            Some(t.GetElementType())
        else
            None

    /// Unwraps <c>Nullable&lt;T&gt;</c> (value-type nullable), returning the inner type if present.
    let private unwrapNullable (t: System.Type) =
        let u = System.Nullable.GetUnderlyingType(t)
        if u <> null then Some u else None

    /// Returns the core type stripped of <c>option</c> and <c>Nullable&lt;&gt;</c> wrappers.
    let private coreType (t: System.Type) =
        match unwrapOption t with
        | Some inner -> inner
        | None ->
            match unwrapNullable t with
            | Some inner -> inner
            | None -> t

    /// Recursively converts a .NET type to a GraphQL type string.
    /// List types become <c>[ElementType]</c>. Option/Nullable wrappers are stripped
    /// (nullability is tracked separately via <see cref="isNullable"/>).
    let rec toGraphQLType (t: System.Type) : string =
        match unwrapOption t with
        | Some inner -> toGraphQLType inner
        | None ->
            match unwrapNullable t with
            | Some inner -> toGraphQLType inner
            | None ->
                match unwrapList t with
                | Some inner -> sprintf "[%s]" (toGraphQLType inner)
                | None ->
                    match t with
                    | t when t = typeof<int> || t = typeof<int64> || t = typeof<int16> || t = typeof<int32> -> "Int"
                    | t when t = typeof<float> || t = typeof<double> || t = typeof<float32> -> "Float"
                    | t when t = typeof<decimal> -> "Float"
                    | t when t = typeof<bool> -> "Boolean"
                    | t when t = typeof<string> -> "String"
                    | t when t = typeof<System.Guid> -> "ID"
                    | t when t = typeof<System.DateTime> -> "DateTime"
                    | t when t = typeof<System.DateTimeOffset> -> "DateTime"
                    | t when t = typeof<System.Object> -> "String"
                    | _ -> t.Name

    /// Returns true when the type is nullable (i.e., wrapped in <c>option</c> or <c>Nullable&lt;&gt;</c>).
    let isNullable (t: System.Type) : bool =
        unwrapOption t |> Option.isSome || unwrapNullable t |> Option.isSome

    /// Returns the GraphQL type string and nullability as a single pair.
    let toGraphQLTypeWithNullability (t: System.Type) : string * bool =
        toGraphQLType t, isNullable t

    /// Converts a .NET type to the <see cref="GraphQLScalar"/> discriminated union.
    let toGraphQLScalar (t: System.Type) : GraphQLScalar =
        let core = coreType t

        match core with
        | t when t = typeof<int> || t = typeof<int64> || t = typeof<int16> || t = typeof<int32> -> GqlInt
        | t when t = typeof<float> || t = typeof<double> || t = typeof<float32> -> GqlFloat
        | t when t = typeof<decimal> -> GqlFloat
        | t when t = typeof<bool> -> GqlBoolean
        | t when t = typeof<string> -> GqlString
        | t when t = typeof<System.Guid> -> GqlId
        | t when t = typeof<System.DateTime> -> GqlDateTime
        | t when t = typeof<System.DateTimeOffset> -> GqlDateTime
        | t -> GqlCustom t.Name

    /// Converts a PascalCase or camelCase string to snake_case.
    /// For example, <c>"SqlSource"</c> becomes <c>"sql_source"</c>.
    let toSnakeCase (s: string) : string =
        if System.String.IsNullOrEmpty(s) then
            s
        else
            let sb = System.Text.StringBuilder()

            for i in 0 .. s.Length - 1 do
                let c = s.[i]

                if System.Char.IsUpper(c) && i > 0 then
                    sb.Append('_') |> ignore
                    sb.Append(System.Char.ToLowerInvariant(c)) |> ignore
                else
                    sb.Append(System.Char.ToLowerInvariant(c)) |> ignore

            sb.ToString()
