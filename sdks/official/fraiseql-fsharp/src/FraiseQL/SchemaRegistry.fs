namespace FraiseQL

open System
open System.Reflection
open System.Collections.Concurrent

/// Thread-safe global registry that accumulates <see cref="TypeDefinition"/>,
/// <see cref="QueryDefinition"/>, and <see cref="MutationDefinition"/> values
/// from attribute-decorated .NET types.
///
/// Use <see cref="register"/> to add types discovered via reflection, then call
/// <see cref="toIntermediateSchema"/> to collect everything into an
/// <see cref="IntermediateSchema"/> for export.
///
/// Call <see cref="reset"/> between test runs to clear accumulated state.
module SchemaRegistry =

    let private types = ConcurrentDictionary<string, TypeDefinition>()
    let private inputTypes = ConcurrentDictionary<string, InputTypeDefinition>()
    let private queries = System.Collections.Generic.List<QueryDefinition>()
    let private mutations = System.Collections.Generic.List<MutationDefinition>()
    let private lockObj = obj ()

    /// Clears all registered types, input types, queries, and mutations. Required between test runs.
    let reset () =
        types.Clear()
        inputTypes.Clear()

        lock lockObj (fun () ->
            queries.Clear()
            mutations.Clear())

    /// Reflects the fields of a type that carries <see cref="GraphQLFieldAttribute"/>.
    let private reflectFields (t: Type) : FieldDefinition list =
        t.GetProperties(BindingFlags.Public ||| BindingFlags.Instance)
        |> Array.choose (fun prop ->
            prop.GetCustomAttribute<GraphQLFieldAttribute>()
            |> Option.ofObj
            |> Option.map (fun fieldAttr ->
                let gqlType, autoNullable =
                    TypeMapper.toGraphQLTypeWithNullability prop.PropertyType

                let resolvedType = if fieldAttr.Type <> "" then fieldAttr.Type else gqlType

                // When an explicit Nullable value was set on the attribute, honour it.
                // Fall back to the .NET type analysis only when no attribute-level type
                // override is present AND the attribute Nullable is still at the default
                // (true), which suggests the developer left it as-is and the .NET type
                // (e.g. option<T>) should drive nullability. When an explicit Type string
                // is provided, always use the attribute's Nullable directly.
                let resolvedNullable =
                    if fieldAttr.Type <> "" then
                        fieldAttr.Nullable
                    elif autoNullable then
                        // .NET type is option<T> or Nullable<T> — treat as nullable regardless
                        true
                    else
                        // .NET type is non-nullable but the attribute may override to true
                        fieldAttr.Nullable

                let scope =
                    if fieldAttr.Scope <> "" then Some fieldAttr.Scope else None

                {
                    name = TypeMapper.toSnakeCase prop.Name
                    type_ = resolvedType
                    nullable = resolvedNullable
                    description =
                        if fieldAttr.Description <> "" then
                            Some fieldAttr.Description
                        else
                            None
                    scope = scope
                    computed = fieldAttr.Computed
                }))
        |> Array.toList

    /// Registers a .NET type that carries <see cref="GraphQLTypeAttribute"/>.
    /// Raises <see cref="ArgumentException"/> when the attribute is missing.
    let register (t: Type) : unit =
        let attr =
            t.GetCustomAttribute<GraphQLTypeAttribute>()
            |> Option.ofObj
            |> Option.defaultWith (fun () ->
                raise (
                    ArgumentException(
                        sprintf
                            "Type '%s' does not have [<GraphQLType>] attribute. Only types decorated with [<GraphQLType>] can be registered."
                            t.Name
                    )
                ))

        let name = if attr.Name <> "" then attr.Name else t.Name

        let typeDef: TypeDefinition =
            {
                name = name
                sql_source = attr.SqlSource
                description = if attr.Description <> "" then Some attr.Description else None
                fields = reflectFields t
                is_input = attr.IsInput
                relay = attr.Relay
                is_error = attr.IsError
            }

        types.[name] <- typeDef

        if attr.Crud then
            let crudQueries, crudMutations, crudInputTypes =
                CrudGenerator.generate name typeDef.fields attr.SqlSource attr.Cascade

            for it in crudInputTypes do
                inputTypes.[it.name] <- it

            lock lockObj (fun () ->
                for q in crudQueries do
                    queries.Add(q)

                for m in crudMutations do
                    mutations.Add(m))

    /// Returns the <see cref="TypeDefinition"/> registered under the given name,
    /// or <c>None</c> if no such type has been registered.
    let getTypeDefinition (name: string) : TypeDefinition option =
        match types.TryGetValue(name) with
        | true, td -> Some td
        | _ -> None

    /// Returns all registered <see cref="TypeDefinition"/> values in an unspecified order.
    let getAllTypes () : TypeDefinition list = types.Values |> Seq.toList

    /// Registers an <see cref="InputTypeDefinition"/> directly.
    /// Raises <see cref="ArgumentException"/> when an input type with the same name
    /// is already registered.
    let registerInput (input: InputTypeDefinition) : unit =
        if not (inputTypes.TryAdd(input.name, input)) then
            raise (
                ArgumentException(
                    sprintf
                        "Input type '%s' is already registered. Each name must be unique within a schema."
                        input.name
                )
            )

    /// Returns all registered <see cref="InputTypeDefinition"/> values in an unspecified order.
    let getAllInputTypes () : InputTypeDefinition list =
        inputTypes.Values |> Seq.toList

    /// Registers a <see cref="QueryDefinition"/> directly (without reflection).
    let registerQuery (q: QueryDefinition) : unit =
        lock lockObj (fun () -> queries.Add(q))

    /// Registers a <see cref="MutationDefinition"/> directly (without reflection).
    let registerMutation (m: MutationDefinition) : unit =
        lock lockObj (fun () -> mutations.Add(m))

    /// Returns all registered <see cref="QueryDefinition"/> values in registration order.
    let getAllQueries () : QueryDefinition list =
        lock lockObj (fun () -> queries |> Seq.toList)

    /// Returns all registered <see cref="MutationDefinition"/> values in registration order.
    let getAllMutations () : MutationDefinition list =
        lock lockObj (fun () -> mutations |> Seq.toList)

    /// Assembles all registered definitions into an <see cref="IntermediateSchema"/> value.
    let toIntermediateSchema () : IntermediateSchema =
        {
            version = "2.0.0"
            types = getAllTypes ()
            input_types = getAllInputTypes ()
            queries = getAllQueries ()
            mutations = getAllMutations ()
        }
