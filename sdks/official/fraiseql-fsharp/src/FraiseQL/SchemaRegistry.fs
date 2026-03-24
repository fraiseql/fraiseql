namespace FraiseQL

open System
open System.Reflection
open System.Collections.Concurrent
open System.Collections.Generic

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
    let private queries = System.Collections.Generic.List<QueryDefinition>()
    let private mutations = System.Collections.Generic.List<MutationDefinition>()
    let private lockObj = obj ()

    let private injectDefaultsBase = Dictionary<string, string>()
    let private injectDefaultsQueries = Dictionary<string, string>()
    let private injectDefaultsMutations = Dictionary<string, string>()

    /// Sets inject defaults for base, queries, and mutations.
    /// Keys are parameter names; values are default expressions.
    let setInjectDefaults
        (baseDefaults: IDictionary<string, string>)
        (queryDefaults: IDictionary<string, string>)
        (mutationDefaults: IDictionary<string, string>)
        : unit =
        lock lockObj (fun () ->
            injectDefaultsBase.Clear()
            injectDefaultsQueries.Clear()
            injectDefaultsMutations.Clear()

            for kv in baseDefaults do
                injectDefaultsBase.[kv.Key] <- kv.Value

            for kv in queryDefaults do
                injectDefaultsQueries.[kv.Key] <- kv.Value

            for kv in mutationDefaults do
                injectDefaultsMutations.[kv.Key] <- kv.Value)

    /// Returns the current inject defaults as (base, queries, mutations) dictionaries.
    let getInjectDefaults () : IDictionary<string, string> * IDictionary<string, string> * IDictionary<string, string> =
        lock lockObj (fun () ->
            (injectDefaultsBase :> IDictionary<_, _>,
             injectDefaultsQueries :> IDictionary<_, _>,
             injectDefaultsMutations :> IDictionary<_, _>))

    /// Clears all registered types, queries, and mutations. Required between test runs.
    let reset () =
        types.Clear()

        lock lockObj (fun () ->
            queries.Clear()
            mutations.Clear()
            injectDefaultsBase.Clear()
            injectDefaultsQueries.Clear()
            injectDefaultsMutations.Clear())

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
                }))
        |> Array.toList

    /// Converts a PascalCase name to snake_case.
    let private pascalToSnake (name: string) =
        System.Text.RegularExpressions.Regex.Replace(name, "(?<!^)([A-Z])", "_$1").ToLowerInvariant()

    /// Pluralizes a snake_case name using basic English rules.
    ///
    /// Rules (ordered):
    /// 1. Already ends in 's' (but not 'ss') → no change (e.g. 'statistics')
    /// 2. Ends in 'ss', 'sh', 'ch', 'x', 'z' → append 'es'
    /// 3. Ends in consonant + 'y' → replace 'y' with 'ies'
    /// 4. Default → append 's'
    let private pluralize (name: string) =
        if name.EndsWith("s") && not (name.EndsWith("ss")) then name
        elif name.EndsWith("ss") || name.EndsWith("sh") || name.EndsWith("ch")
             || name.EndsWith("x") || name.EndsWith("z") then name + "es"
        elif name.Length >= 2 && name.EndsWith("y")
             && not ("aeiou".Contains(name.[name.Length - 2])) then name.[.. name.Length - 2] + "ies"
        else name + "s"

    /// Generates CRUD queries and mutations for a registered type.
    let private generateCrud (typeDef: TypeDefinition) (crudOps: string[]) : unit =
        let snake = pascalToSnake typeDef.name
        let view = if System.String.IsNullOrEmpty(typeDef.sql_source) then "v_" + snake else typeDef.sql_source

        let expandedOps =
            crudOps
            |> Array.collect (fun op ->
                if op = "all" then [| "read"; "create"; "update"; "delete" |]
                else [| op |])
            |> Array.distinct

        let pkFields =
            typeDef.fields
            |> List.filter (fun f -> f.name.StartsWith("pk_") || f.name = "id" || f.type_ = "ID")

        let nonPkFields =
            typeDef.fields
            |> List.filter (fun f -> not (f.name.StartsWith("pk_") || f.name = "id" || f.type_ = "ID"))

        for op in expandedOps do
            match op with
            | "read" ->
                // get by ID query
                let getArgs =
                    pkFields
                    |> List.map (fun f -> { name = f.name; type_ = f.type_; nullable = false }: ArgumentDefinition)

                let getQuery: QueryDefinition =
                    {
                        name = sprintf "get%s" typeDef.name
                        return_type = typeDef.name
                        returns_list = false
                        nullable = true
                        sql_source = view
                        arguments = getArgs
                        cache_ttl_seconds = None
                        description = Some(sprintf "Get a single %s by primary key." typeDef.name)
                        rest_path = None
                        rest_method = None
                    }

                lock lockObj (fun () -> queries.Add(getQuery))

                // list query
                let listQuery: QueryDefinition =
                    {
                        name = sprintf "list%s" (pluralize typeDef.name)
                        return_type = typeDef.name
                        returns_list = true
                        nullable = false
                        sql_source = view
                        arguments = []
                        cache_ttl_seconds = None
                        description = Some(sprintf "List all %s records." typeDef.name)
                        rest_path = None
                        rest_method = None
                    }

                lock lockObj (fun () -> queries.Add(listQuery))

            | "create" ->
                let fnName = sprintf "fn_create_%s" snake

                let createArgs =
                    nonPkFields
                    |> List.map (fun f -> { name = f.name; type_ = f.type_; nullable = f.nullable }: ArgumentDefinition)

                let createMut: MutationDefinition =
                    {
                        name = sprintf "create%s" typeDef.name
                        return_type = typeDef.name
                        sql_source = fnName
                        operation = "insert"
                        arguments = createArgs
                        description = Some(sprintf "Create a new %s." typeDef.name)
                        rest_path = None
                        rest_method = None
                    }

                lock lockObj (fun () -> mutations.Add(createMut))

            | "update" ->
                let fnName = sprintf "fn_update_%s" snake

                let pkArgs =
                    pkFields
                    |> List.map (fun f -> { name = f.name; type_ = f.type_; nullable = false }: ArgumentDefinition)

                let nonPkArgs =
                    nonPkFields
                    |> List.map (fun f -> { name = f.name; type_ = f.type_; nullable = true }: ArgumentDefinition)

                let updateArgs = pkArgs @ nonPkArgs

                let updateMut: MutationDefinition =
                    {
                        name = sprintf "update%s" typeDef.name
                        return_type = typeDef.name
                        sql_source = fnName
                        operation = "update"
                        arguments = updateArgs
                        description = Some(sprintf "Update an existing %s." typeDef.name)
                        rest_path = None
                        rest_method = None
                    }

                lock lockObj (fun () -> mutations.Add(updateMut))

            | "delete" ->
                let fnName = sprintf "fn_delete_%s" snake

                let deleteArgs =
                    pkFields
                    |> List.map (fun f -> { name = f.name; type_ = f.type_; nullable = false }: ArgumentDefinition)

                let deleteMut: MutationDefinition =
                    {
                        name = sprintf "delete%s" typeDef.name
                        return_type = typeDef.name
                        sql_source = fnName
                        operation = "delete"
                        arguments = deleteArgs
                        description = Some(sprintf "Delete a %s by primary key." typeDef.name)
                        rest_path = None
                        rest_method = None
                    }

                lock lockObj (fun () -> mutations.Add(deleteMut))

            | _ -> ()

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
                tenant_scoped = attr.TenantScoped
                key_fields = attr.KeyFields
                extends_type = attr.Extends
            }

        types.[name] <- typeDef

        if attr.Crud.Length > 0 then
            generateCrud typeDef attr.Crud

    /// Returns the <see cref="TypeDefinition"/> registered under the given name,
    /// or <c>None</c> if no such type has been registered.
    let getTypeDefinition (name: string) : TypeDefinition option =
        match types.TryGetValue(name) with
        | true, td -> Some td
        | _ -> None

    /// Returns all registered <see cref="TypeDefinition"/> values in an unspecified order.
    let getAllTypes () : TypeDefinition list = types.Values |> Seq.toList

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
            queries = getAllQueries ()
            mutations = getAllMutations ()
        }
