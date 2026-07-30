namespace FraiseQL

/// Pipe-friendly builder for constructing <see cref="MutationDefinition"/> values.
///
/// Example:
/// <code>
/// MutationBuilder.mutation "createAuthor"
/// |> MutationBuilder.returnType "Author"
/// |> MutationBuilder.sqlSource "fn_create_author"
/// |> MutationBuilder.operation "insert"
/// |> MutationBuilder.register
/// </code>
module MutationBuilder =

    /// Internal accumulator state for building a <see cref="MutationDefinition"/>.
    type MutationState =
        {
            name: string
            returnType: string
            sqlSource: string
            operation: string
            arguments: ArgumentDefinition list
            description: string option
            rest: RestConfig option
            injectParams: Map<string, string>
            requiresRole: string option
            invalidatesViews: string list
            invalidatesFactTables: string list
        }

    /// Creates a new <see cref="MutationState"/> for the given mutation name.
    let mutation (name: string) : MutationState =
        {
            name = name
            returnType = ""
            sqlSource = ""
            operation = "custom"
            arguments = []
            description = None
            rest = None
            injectParams = Map.empty
            requiresRole = None
            invalidatesViews = []
            invalidatesFactTables = []
        }

    /// Sets the GraphQL return type for this mutation.
    let returnType (t: string) (s: MutationState) : MutationState = { s with returnType = t }

    /// Sets the SQL function backing this mutation.
    let sqlSource (src: string) (s: MutationState) : MutationState = { s with sqlSource = src }

    /// Sets the operation kind: "insert", "update", "delete", or "custom".
    let operation (op: string) (s: MutationState) : MutationState = { s with operation = op }

    /// Sets the optional human-readable description.
    let description (d: string) (s: MutationState) : MutationState =
        { s with description = Some d }

    /// Adds an argument to this mutation.
    let withArgument (name: string) (type_: string) (isNullable: bool) (s: MutationState) : MutationState =
        let arg: ArgumentDefinition = { name = name; type_ = type_; nullable = isNullable }
        { s with arguments = s.arguments @ [ arg ] }

    /// Sets the optional REST endpoint annotation.
    let rest (cfg: RestConfig) (s: MutationState) : MutationState = { s with rest = Some cfg }

    /// Declares a server-injected parameter, not exposed as a GraphQL argument.
    /// `source` is of the form `"jwt:&lt;claim&gt;"`.
    let inject (parameter: string) (source: string) (s: MutationState) : MutationState =
        { s with injectParams = Map.add parameter source s.injectParams }

    /// Restricts this mutation to callers holding the given role.
    let requiresRole (role: string) (s: MutationState) : MutationState =
        { s with requiresRole = Some role }

    /// Declares views whose cached query results must be invalidated after this mutation.
    let invalidatesViews (views: string list) (s: MutationState) : MutationState =
        { s with invalidatesViews = s.invalidatesViews @ views }

    /// Declares fact tables whose cached aggregates must be invalidated after this mutation.
    let invalidatesFactTables (tables: string list) (s: MutationState) : MutationState =
        { s with invalidatesFactTables = s.invalidatesFactTables @ tables }

    /// Converts the accumulated state into a <see cref="MutationDefinition"/>.
    /// Raises <see cref="System.InvalidOperationException"/> when required fields are missing.
    let toDefinition (s: MutationState) : MutationDefinition =
        if s.returnType = "" then
            raise (
                System.InvalidOperationException(sprintf "Mutation '%s' has no returnType" s.name)
            )

        if s.sqlSource = "" then
            raise (
                System.InvalidOperationException(sprintf "Mutation '%s' has no sqlSource" s.name)
            )

        {
            name = s.name
            return_type = s.returnType
            sql_source = s.sqlSource
            operation = s.operation
            arguments = s.arguments
            description = s.description
            rest = s.rest
            cascade = None
            inject_params = (if Map.isEmpty s.injectParams then None else Some s.injectParams)
            requires_role = s.requiresRole
            invalidates_views =
                (if List.isEmpty s.invalidatesViews then None else Some s.invalidatesViews)
            invalidates_fact_tables =
                (if List.isEmpty s.invalidatesFactTables then None else Some s.invalidatesFactTables)
        }

    /// Converts the state to a <see cref="MutationDefinition"/> and registers it in <see cref="SchemaRegistry"/>.
    let register (s: MutationState) : unit = SchemaRegistry.registerMutation (toDefinition s)
