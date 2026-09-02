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
    let private enums = System.Collections.Generic.List<EnumDefinition>()
    let private lockObj = obj ()

    /// Clears all registered types, input types, queries, and mutations. Required between test runs.
    let reset () =
        types.Clear()
        inputTypes.Clear()

        lock lockObj (fun () ->
            queries.Clear()
            mutations.Clear()
            enums.Clear())

    /// Reflects the fields of a type that carries <see cref="GraphQLFieldAttribute"/>.
    let private reflectFields (t: Type) : FieldDefinition list =
        t.GetProperties(BindingFlags.Public ||| BindingFlags.Instance)
        |> Array.choose (fun prop ->
            prop.GetCustomAttribute<GraphQLFieldAttribute>()
            |> Option.ofObj
            |> Option.map (fun fieldAttr ->
                let gqlType, autoNullable =
                    TypeMapper.toGraphQLTypeWithNullability prop.PropertyType

                // camelCase on the way out (#1249). This was `toSnakeCase`, so an F#
                // schema published `last_login_at` where every other SDK published
                // `lastLoginAt` for the same declaration.
                let fieldName = TypeMapper.toCamelCase prop.Name

                // An explicit attribute Type override wins over the inferred .NET type.
                let resolvedTypeRaw = if fieldAttr.Type <> "" then fieldAttr.Type else gqlType

                // Entity-identity contract (ADR-0017): a field named `id` is emitted as
                // `ID`. Both `string` (→ "String") and the explicit `UUID` scalar are
                // wire-transparent string representations of an identity, so an `id`
                // typed either way is canonicalized to `ID` at authoring time — keeping
                // the emitted schema honest instead of leaking `id: String`, which the
                // compiler would reject. A numeric `id: Int` is left unchanged.
                // Canonicalization runs *after* any explicit Type override, so an
                // inferred string id and `[<GraphQLField(Type = "String")>]` both end at
                // `ID`, matching the Python SDK's `_canonicalize_id_type`.
                let resolvedType =
                    if fieldName = "id" && (resolvedTypeRaw = "String" || resolvedTypeRaw = "UUID") then
                        "ID"
                    else
                        resolvedTypeRaw

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

                // A field is either an embedding or the Float reporting how far a
                // search's result was from the query vector, and a column has at least
                // one dimension. Which metrics a field type admits and which index types
                // have an operator class for them depends on pgvector's own tables, and
                // is checked once, in the compiler.
                let declaresVector =
                    fieldAttr.VectorDimensions <> 0
                    || fieldAttr.VectorIndexType <> ""
                    || fieldAttr.VectorDistanceMetric <> ""

                if declaresVector && fieldAttr.VectorDistance <> "" then
                    invalidArg
                        "VectorDistance"
                        (sprintf
                            "Field '%s' declares both a vector config and a vector distance; a field is either an embedding or the Float reporting a search's distance, not both"
                            fieldName)

                if declaresVector && fieldAttr.VectorDimensions < 1 then
                    invalidArg
                        "VectorDimensions"
                        (sprintf
                            "Field '%s' declares %d vector dimensions; dimensions must be at least 1"
                            fieldName
                            fieldAttr.VectorDimensions)

                // The index type and the metric are written out even where the author
                // left them off, so the emitted schema says which index and which metric
                // the column will get rather than leaving it to a compiler default the
                // author cannot see.
                let vectorConfig =
                    if declaresVector then
                        Some
                            {
                                dimensions = fieldAttr.VectorDimensions
                                index_type =
                                    if fieldAttr.VectorIndexType <> "" then
                                        fieldAttr.VectorIndexType
                                    else
                                        VectorIndex.hnsw
                                distance_metric =
                                    if fieldAttr.VectorDistanceMetric <> "" then
                                        fieldAttr.VectorDistanceMetric
                                    else
                                        VectorMetric.cosine
                            }
                    else
                        None

                // `vector_distance` names a sibling *field*, so it goes through the same
                // conversion the field name above goes through — or the reference names a
                // field that exists in the schema under a different spelling (#1249).
                let vectorDistance =
                    if fieldAttr.VectorDistance <> "" then
                        Some(TypeMapper.toCamelCase fieldAttr.VectorDistance)
                    else
                        None

                // `Deprecated` and `DeprecationReason` have been on the attribute all
                // along and were read nowhere, so an author who marked a field deprecated
                // got a schema that says nothing about it. A reason with no `Deprecated`
                // flag still deprecates: writing one is unambiguous intent.
                let deprecated =
                    if fieldAttr.Deprecated || fieldAttr.DeprecationReason <> "" then
                        Some
                            {
                                reason =
                                    if fieldAttr.DeprecationReason <> "" then
                                        Some fieldAttr.DeprecationReason
                                    else
                                        None
                            }
                    else
                        None

                {
                    name = fieldName
                    type_ = resolvedType
                    nullable = resolvedNullable
                    description =
                        if fieldAttr.Description <> "" then
                            Some fieldAttr.Description
                        else
                            None
                    scope = scope
                    computed = fieldAttr.Computed
                    vector_config = vectorConfig
                    vector_distance = vectorDistance
                    deprecated = deprecated
                }))
        |> Array.toList

    /// Registers a .NET type that carries <see cref="GraphQLTypeAttribute"/>.
    /// Raises <see cref="ArgumentException"/> when the attribute is missing.
    /// Reads and checks one type's [<GraphQLRelationship>] attributes (#1266).
    ///
    /// Only the shape this SDK owns is checked — a blank key, an unknown cardinality, and
    /// a name declared twice, which no compiler diagnostic can attribute back to an
    /// attribute. Whether a relationship can be *followed* is checked by the compiler
    /// against the whole schema, which is the only place that knows.
    let private reflectRelationships (typeName: string) (t: Type) : RelationshipDefinition list =
        let attrs =
            t.GetCustomAttributes(typeof<GraphQLRelationshipAttribute>, false)
            |> Array.map (fun a -> a :?> GraphQLRelationshipAttribute)
            |> Array.toList

        let mutable seen = Set.empty

        attrs
        |> List.map (fun attr ->
            for label, value in
                [ "Name", attr.Name
                  "TargetType", attr.TargetType
                  "ForeignKey", attr.ForeignKey
                  "ReferencedKey", attr.ReferencedKey ] do
                if String.IsNullOrEmpty value then
                    raise (
                        ArgumentException(
                            sprintf "Type '%s': relationship %s must not be empty" typeName label
                        )
                    )

            if not (List.contains attr.Cardinality Cardinality.all) then
                raise (
                    ArgumentException(
                        sprintf
                            "Type '%s': relationship '%s' cardinality must be one of %s (got '%s')"
                            typeName
                            attr.Name
                            (String.Join(", ", Cardinality.all))
                            attr.Cardinality
                    )
                )

            if Set.contains attr.Name seen then
                raise (
                    ArgumentException(
                        sprintf
                            "Type '%s': relationship '%s' is declared more than once; an embed resolves the first and the rest are unreachable"
                            typeName
                            attr.Name
                    )
                )

            seen <- Set.add attr.Name seen

            {
                name = attr.Name
                target_type = attr.TargetType
                cardinality = attr.Cardinality
                foreign_key = attr.ForeignKey
                referenced_key = attr.ReferencedKey
            })

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
                relationships = reflectRelationships name t
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
    /// Registers a GraphQL enum type.
    let registerEnum (name: string) (values: string list) (description: string option) : unit =
        lock lockObj (fun () ->
            enums.Add(
                {
                    name = name
                    values = values |> List.map (fun v -> { name = v })
                    description = description
                }
            ))

    /// Returns every registered enum type.
    let getAllEnums () : EnumDefinition list =
        lock lockObj (fun () -> enums |> Seq.toList)

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
            enums = getAllEnums ()
            queries = getAllQueries ()
            mutations = getAllMutations ()
        }
