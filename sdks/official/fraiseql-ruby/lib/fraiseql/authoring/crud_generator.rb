# frozen_string_literal: true

module FraiseQL
  # Generates CRUD queries and mutations for FraiseQL types.
  #
  # When a type has `fraiseql_crud true`, this module produces standard read,
  # create, update, and delete operations following FraiseQL conventions:
  #
  # - Read: query `{snake}` (get by PK) + query `{snakes}` (list with auto_params)
  # - Create: mutation `create_{snake}` with all fields
  # - Update: mutation `update_{snake}` with PK required, other fields nullable
  # - Delete: mutation `delete_{snake}` with PK only
  module CrudGenerator
    module_function

    # Convert a PascalCase name to snake_case.
    #
    # @param name [String] the PascalCase name
    # @return [String] the snake_case equivalent
    def pascal_to_snake(name)
      name.gsub(/(?<!^)([A-Z])/, '_\1').downcase
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
    # @return [Hash] with :queries and :mutations arrays
    # @raise [ArgumentError] if fields is empty
    def generate(type_name:, fields:, sql_source: nil, cascade: false)
      raise ArgumentError, "Type \"#{type_name}\" has no fields; cannot generate CRUD operations" if fields.empty?

      snake = pascal_to_snake(type_name)
      view = sql_source || "v_#{snake}"
      pk = fields.first

      queries = []
      mutations = []

      # Get by ID
      queries << {
        name: snake,
        return_type: type_name,
        returns_list: false,
        nullable: true,
        arguments: [{ name: pk[:name], type: pk[:type], nullable: false }],
        description: "Get #{type_name} by ID.",
        sql_source: view
      }

      # List
      queries << {
        name: pluralize(snake),
        return_type: type_name,
        returns_list: true,
        nullable: false,
        arguments: [],
        description: "List #{type_name} records.",
        sql_source: view,
        auto_params: { where: true, order_by: true, limit: true, offset: true }
      }

      # Create
      create = {
        name: "create_#{snake}",
        return_type: type_name,
        returns_list: false,
        nullable: false,
        arguments: fields.map { |f| { name: f[:name], type: f[:type], nullable: f[:nullable] } },
        description: "Create a new #{type_name}.",
        sql_source: "fn_create_#{snake}",
        operation: "INSERT"
      }
      create[:cascade] = true if cascade
      mutations << create

      # Update
      update = {
        name: "update_#{snake}",
        return_type: type_name,
        returns_list: false,
        nullable: true,
        arguments: [{ name: pk[:name], type: pk[:type], nullable: false }] +
          fields[1..].map { |f| { name: f[:name], type: f[:type], nullable: true } },
        description: "Update an existing #{type_name}.",
        sql_source: "fn_update_#{snake}",
        operation: "UPDATE"
      }
      update[:cascade] = true if cascade
      mutations << update

      # Delete
      delete = {
        name: "delete_#{snake}",
        return_type: type_name,
        returns_list: false,
        nullable: false,
        arguments: [{ name: pk[:name], type: pk[:type], nullable: false }],
        description: "Delete a #{type_name}.",
        sql_source: "fn_delete_#{snake}",
        operation: "DELETE"
      }
      delete[:cascade] = true if cascade
      mutations << delete

      { queries: queries, mutations: mutations }
    end
  end
end
