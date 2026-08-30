# frozen_string_literal: true

require_relative "../naming"

module FraiseQL
  # Generates CRUD queries and mutations for FraiseQL types.
  #
  # When a type has `fraiseql_crud true`, this module produces standard read,
  # create, update, and delete operations following FraiseQL conventions:
  #
  # - Read: query `{snake}` (get by PK) + query `{snakes}` (list with auto_params)
  # - Create: mutation `create_{snake}` taking `input: Create{Type}Input!`
  # - Update: mutation `update_{snake}` taking `input: Update{Type}Input!`
  # - Delete: mutation `delete_{snake}` with PK only
  #
  # The two input objects are the shape six of the nine generating SDKs emit and the one
  # `docs/architecture/mutation-response.md` documents. This generator used to emit flat
  # arguments and no input types, so the same `crud` declaration produced a different
  # GraphQL API in Ruby than in Python (#1246). A flat-argument mutation also changes its
  # signature every time the type gains a column; an input object absorbs it.
  module CrudGenerator
    module_function

    # Convert a PascalCase name to snake_case.
    #
    # @param name [String] the PascalCase name
    # @return [String] the snake_case equivalent
    def pascal_to_snake(name)
      name.gsub(/(?<!^)([A-Z])/, '_\1').downcase
    end

    # Convert a snake_case name to camelCase.
    #
    # Delegates to {FraiseQL::Naming.snake_to_camel} — the one implementation the type
    # builder, the `Type` mixin and this generator all share, so a type's fields and its
    # generated input objects cannot disagree about the name of the same column (#1249).
    # This method stays because it is public surface and has its own test.
    #
    # @param name [String] the snake_case name
    # @return [String] the camelCase equivalent
    def snake_to_camel(name)
      FraiseQL::Naming.snake_to_camel(name)
    end

    # Apply basic English pluralization rules to a snake_case name.
    #
    # Rules (ordered):
    # 1. Already ends in 's' (but not 'ss') -> no change
    # 2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
    # 3. Ends in consonant + 'y' -> replace 'y' with 'ies'
    # 4. Default -> append 's'
    #
    # @param name [String] the singular name
    # @return [String] the pluralized name
    def pluralize(name)
      return name if name.end_with?("s") && !name.end_with?("ss")
      return "#{name}es" if %w[ss sh ch x z].any? { |s| name.end_with?(s) }

      if name.length >= 2 && name[-1] == "y" && !"aeiou".include?(name[-2])
        return "#{name[0..-2]}ies"
      end

      "#{name}s"
    end

    # Generate CRUD operations for a type.
    #
    # @param type_name [String] the PascalCase GraphQL type name
    # @param fields [Array<Hash>] field definitions with :name, :type, :nullable keys
    # @param sql_source [String, nil] override for the default view name
    # @param cascade [Boolean] when true, generated mutations include cascade: true
    # @return [Hash] with :queries, :mutations and :input_types arrays
    # @raise [ArgumentError] if fields is empty
    def generate(type_name:, fields:, sql_source: nil, cascade: false)
      raise ArgumentError, "Type \"#{type_name}\" has no fields; cannot generate CRUD operations" if fields.empty?

      snake = pascal_to_snake(type_name)
      view = sql_source || "v_#{snake}"
      pk = fields.first

      queries = []
      mutations = []
      input_types = []

      # Get by ID
      queries << {
        name: snake_to_camel(snake),
        return_type: type_name,
        returns_list: false,
        nullable: true,
        arguments: [{ name: snake_to_camel(pk[:name]), type: pk[:type], nullable: false }],
        description: "Get #{type_name} by ID.",
        sql_source: view
      }

      # List
      queries << {
        name: snake_to_camel(pluralize(snake)),
        return_type: type_name,
        returns_list: true,
        nullable: false,
        arguments: [],
        description: "List #{type_name} records.",
        sql_source: view,
        auto_params: { where: true, order_by: true, limit: true, offset: true }
      }

      # Create — every non-computed field, in an input object. A computed field is
      # server-assigned (a slug, a view aggregation), so a client cannot supply one.
      create_input_name = "Create#{type_name}Input"
      input_types << {
        name: create_input_name,
        description: "Input for creating a new #{type_name}.",
        fields: fields.reject { |f| f[:computed] }
                      .map { |f| { name: snake_to_camel(f[:name]), type: f[:type], nullable: f[:nullable] } }
      }
      create = {
        name: snake_to_camel("create_#{snake}"),
        return_type: type_name,
        returns_list: false,
        nullable: false,
        arguments: [{ name: "input", type: create_input_name, nullable: false }],
        description: "Create a new #{type_name}.",
        sql_source: "fn_create_#{snake}",
        operation: "INSERT"
      }
      create[:cascade] = true if cascade
      mutations << create

      # Update — PK required, every other non-computed field optional, in an input object.
      update_input_name = "Update#{type_name}Input"
      input_types << {
        name: update_input_name,
        description: "Input for updating an existing #{type_name}.",
        fields: [{ name: snake_to_camel(pk[:name]), type: pk[:type], nullable: false }] +
                fields[1..].reject { |f| f[:computed] }
                           .map { |f| { name: snake_to_camel(f[:name]), type: f[:type], nullable: true } }
      }
      update = {
        name: snake_to_camel("update_#{snake}"),
        return_type: type_name,
        returns_list: false,
        nullable: true,
        arguments: [{ name: "input", type: update_input_name, nullable: false }],
        description: "Update an existing #{type_name}.",
        sql_source: "fn_update_#{snake}",
        operation: "UPDATE"
      }
      update[:cascade] = true if cascade
      mutations << update

      # Delete
      delete = {
        name: snake_to_camel("delete_#{snake}"),
        return_type: type_name,
        returns_list: false,
        nullable: false,
        arguments: [{ name: snake_to_camel(pk[:name]), type: pk[:type], nullable: false }],
        description: "Delete a #{type_name}.",
        sql_source: "fn_delete_#{snake}",
        operation: "DELETE"
      }
      delete[:cascade] = true if cascade
      mutations << delete

      { queries: queries, mutations: mutations, input_types: input_types }
    end
  end
end
