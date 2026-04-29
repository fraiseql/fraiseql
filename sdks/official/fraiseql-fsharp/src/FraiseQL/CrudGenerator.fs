namespace FraiseQL

open System.Text.RegularExpressions

/// Generates standard CRUD queries and mutations for a GraphQL type.
///
/// Generated operations follow FraiseQL conventions:
///   - Read:   query <snake> (get by PK) + query <snake>s (list with auto_params)
///   - Create: mutation create_<snake> with a Create<Type>Input input object
///   - Update: mutation update_<snake> with an Update<Type>Input input object
///   - Delete: mutation delete_<snake> with PK only
module CrudGenerator =

    let private camelRe = Regex(@"(?<!^)([A-Z])", RegexOptions.Compiled)

    /// Converts a PascalCase name to snake_case.
    let pascalToSnake (name: string) =
        camelRe.Replace(name, "_$1").ToLowerInvariant()

    /// Applies basic English pluralization rules to a snake_case name.
    let pluralize (name: string) =
        if name.EndsWith("s") && not (name.EndsWith("ss")) then
            name
        elif
            [ "ss"; "sh"; "ch"; "x"; "z" ]
            |> List.exists name.EndsWith
        then
            name + "es"
        elif
            name.Length >= 2
            && name.[name.Length - 1] = 'y'
            && not ("aeiou".Contains(name.[name.Length - 2]))
        then
            name.[.. name.Length - 2] + "ies"
        else
            name + "s"

    /// Generates CRUD queries, mutations, and input types for the given type.
    ///
    /// Returns a tuple of (queries, mutations, input_types).
    ///
    /// # Errors
    ///
    /// Raises System.InvalidOperationException when the fields list is empty.
    let generate
        (typeName: string)
        (fields: FieldDefinition list)
        (sqlSource: string)
        (cascade: bool)
        : QueryDefinition list * MutationDefinition list * InputTypeDefinition list
        =
        if fields.IsEmpty then
            raise (
                System.InvalidOperationException(
                    sprintf "Type '%s' has no fields; cannot generate CRUD operations" typeName
                )
            )

        let snake = pascalToSnake typeName

        let view =
            if System.String.IsNullOrEmpty(sqlSource) then
                "v_" + snake
            else
                sqlSource

        let pkField = fields.[0]
        let cascadeValue = if cascade then Some true else None

        let createInputName = sprintf "Create%sInput" typeName

        let createInputType: InputTypeDefinition =
            {
                name = createInputName
                fields =
                    fields
                    |> List.filter (fun f -> not f.computed)
                    |> List.map (fun f ->
                        {
                            name = f.name
                            type_ = f.type_
                            nullable = f.nullable
                        })
                description = Some(sprintf "Input for creating a new %s." typeName)
            }

        let updateInputName = sprintf "Update%sInput" typeName

        let updateInputType: InputTypeDefinition =
            {
                name = updateInputName
                fields =
                    { name = pkField.name; type_ = pkField.type_; nullable = false }
                    :: (fields
                        |> List.tail
                        |> List.filter (fun f -> not f.computed)
                        |> List.map (fun f ->
                            {
                                name = f.name
                                type_ = f.type_
                                nullable = true
                            }))
                description = Some(sprintf "Input for updating an existing %s." typeName)
            }

        let queries =
            [
                // Get-by-ID query
                {
                    name = snake
                    return_type = typeName
                    returns_list = false
                    nullable = true
                    sql_source = view
                    sql_source_dispatch = None
                    arguments =
                        [
                            {
                                name = pkField.name
                                type_ = pkField.type_
                                nullable = false
                            }
                        ]
                    cache_ttl_seconds = None
                    description = Some(sprintf "Get %s by ID." typeName)
                    rest = None
                }
                // List query
                {
                    name = pluralize snake
                    return_type = typeName
                    returns_list = true
                    nullable = false
                    sql_source = view
                    sql_source_dispatch = None
                    arguments = []
                    cache_ttl_seconds = None
                    description = Some(sprintf "List %s records." typeName)
                    rest = None
                }
            ]

        let mutations =
            [
                // Create mutation — single input object argument
                {
                    name = "create_" + snake
                    return_type = typeName
                    sql_source = "fn_create_" + snake
                    sql_source_dispatch = None
                    operation = "INSERT"
                    arguments =
                        [
                            {
                                name = "input"
                                type_ = createInputName
                                nullable = false
                            }
                        ]
                    description = Some(sprintf "Create a new %s." typeName)
                    rest = None
                    cascade = cascadeValue
                }
                // Update mutation — single input object argument
                {
                    name = "update_" + snake
                    return_type = typeName
                    sql_source = "fn_update_" + snake
                    sql_source_dispatch = None
                    operation = "UPDATE"
                    arguments =
                        [
                            {
                                name = "input"
                                type_ = updateInputName
                                nullable = false
                            }
                        ]
                    description = Some(sprintf "Update an existing %s." typeName)
                    rest = None
                    cascade = cascadeValue
                }
                // Delete mutation — PK only (unchanged)
                {
                    name = "delete_" + snake
                    return_type = typeName
                    sql_source = "fn_delete_" + snake
                    sql_source_dispatch = None
                    operation = "DELETE"
                    arguments =
                        [
                            {
                                name = pkField.name
                                type_ = pkField.type_
                                nullable = false
                            }
                        ]
                    description = Some(sprintf "Delete a %s." typeName)
                    rest = None
                    cascade = cascadeValue
                }
            ]

        let inputTypes = [ createInputType; updateInputType ]

        (queries, mutations, inputTypes)
