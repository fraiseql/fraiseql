# frozen_string_literal: true

require "json"

require_relative "naming"

module FraiseQL
  # Builds the intermediate `schema.json` document consumed by `fraiseql compile`.
  #
  # This is the API the README's Quick Start has always documented. It did not exist:
  # `FraiseQL::Schema`, `schema.type`, `schema.export_json` were all absent from the gem,
  # and there was no `lib/fraiseql.rb` either, so `require "fraiseql"` raised LoadError on
  # the first line of the documented example (#853). The only working entry point was the
  # per-class `FraiseQL::Type` mixin, whose `to_fraiseql_schema` omitted the required
  # `nullable` key and so could not be compiled anyway (#854).
  #
  # Every key emitted here is the one the compiler reads. The compiler denies unknown
  # fields, so a misspelling is a compile error naming the key rather than a silent drop.
  #
  #   schema = FraiseQL::Schema.new
  #
  #   schema.type "User", sql_source: "v_user" do |t|
  #     t.field :id, :id, nullable: false
  #     t.field :email, :string, nullable: false
  #   end
  #
  #   schema.query "users", return_type: "User", returns_list: true, sql_source: "v_user"
  #
  #   schema.export_json("schema.json")
  class Schema
    # Maps the Ruby type symbols the README uses onto GraphQL type names. Anything not
    # listed is PascalCased, so `:order_status` names the `OrderStatus` enum and a
    # `"User"` string passes through untouched.
    SCALARS = {
      int: "Int",
      integer: "Int",
      float: "Float",
      string: "String",
      boolean: "Boolean",
      bool: "Boolean",
      id: "ID",
      datetime: "DateTime",
      date: "Date",
      time: "Time",
      json: "Json",
      uuid: "UUID",
      decimal: "Decimal",
      vector: "Vector",
      bit_vector: "BitVector",
      half_vector: "HalfVector",
      sparse_vector: "SparseVector"
    }.freeze

    # The `ActorType` roster #966's actor gate is an allow-list of, snake_case as the
    # compiler spells it (`crates/fraiseql-core/src/security/actor_type.rs`) and as the
    # change-log `actor_type TEXT` column stores it.
    ACTOR_TYPES = %w[human_user service_account ai_agent system_job].freeze

    # Index types a pgvector column can be searched through.
    VECTOR_INDEXES = %w[hnsw ivf_flat none].freeze

    # Distance metrics a vector search can order by. Which of them a given field type
    # and index admit is pgvector's business and the compiler's — it holds the
    # operator-class table and refuses a combination that has no class, naming the
    # alternative. This SDK carries no second copy of that table; a copy is what drifts.
    VECTOR_METRICS = %w[cosine l2 inner_product hamming jaccard].freeze

    def initialize
      @types = []
      @enums = []
      @queries = []
      @mutations = []
    end

    # Declares a GraphQL object type backed by a SQL view.
    #
    # `is_input: true` declares a GraphQL input object instead. An input object has no
    # backing relation, so `sql_source` is refused on one — the compiler rejects a type
    # that declares both.
    def type(name, sql_source: nil, description: nil, relay: false, is_error: false, is_input: false,
             crud: false, cascade: false)
      if is_input && sql_source
        raise ArgumentError,
              "type #{name.inspect}: an input type must not declare sql_source — " \
              "an input object has no backing view."
      end
      if crud && is_input
        raise ArgumentError,
              "type #{name.inspect}: an input object has no CRUD operations to generate."
      end

      builder = TypeBuilder.new
      yield builder if block_given?

      definition = { "name" => name.to_s, "fields" => builder.fields }
      definition["sql_source"] = sql_source.to_s if sql_source
      definition["description"] = description if description
      definition["relay"] = true if relay
      definition["is_error"] = true if is_error
      definition["is_input"] = true if is_input

      @types << definition
      expand_crud(name.to_s, builder.crud_fields, sql_source, cascade) if crud
      definition
    end

    # Merges what `CrudGenerator` produces into the document being built.
    #
    # `crud:` used to set no flag this class read at all: `CrudGenerator` was complete and
    # correct and had no caller anywhere in `lib/` — only its own tests called it, which is
    # why a green suite was not evidence that declaring CRUD did anything (#1242). The
    # per-class `FraiseQL::Type` mixin's `to_fraiseql_crud` is in the same position; this
    # builder is the path `conformance/export.rb`, the README Quick Start and
    # `bin/fraiseql` all run, so it is the one that has to work.
    #
    # Input objects are appended to `types` with `is_input`, the spelling the rest of this
    # class uses and the one the conformance fixture proves the compiler accepts.
    def expand_crud(type_name, fields, sql_source, cascade)
      generated = CrudGenerator.generate(
        type_name: type_name, fields: fields, sql_source: sql_source&.to_s, cascade: cascade
      )
      generated[:input_types].each do |input|
        @types << {
          "name" => input[:name],
          "description" => input[:description],
          "is_input" => true,
          "fields" => input[:fields].map do |f|
            { "name" => f[:name], "type" => f[:type], "nullable" => f[:nullable] }
          end
        }
      end
      generated[:queries].each { |q| @queries << stringify_operation(q) }
      generated[:mutations].each { |m| @mutations << stringify_operation(m) }
    end

    # The generator speaks symbols; the document is string-keyed. One conversion, here,
    # rather than a second string-keyed copy of the generator that drifts from it.
    def stringify_operation(op)
      op.each_with_object({}) do |(key, value), out|
        out[key.to_s] =
          case value
          when Array then value.map { |arg| arg.transform_keys(&:to_s) }
          when Hash  then value.transform_keys(&:to_s)
          else value
          end
      end
    end

    # Declares a GraphQL enum type.
    def enum(name, values, description: nil)
      definition = {
        "name" => name.to_s,
        "values" => values.map { |value| { "name" => value.to_s } }
      }
      definition["description"] = description if description

      @enums << definition
      definition
    end

    # Declares a GraphQL query.
    #
    # `inject` maps a SQL parameter to a `"jwt:<claim>"` source and is emitted under
    # `inject_params` — the key the compiler reads.
    def query(name, return_type:, sql_source: nil, returns_list: false, nullable: false,
              description: nil, cache_ttl_seconds: nil, requires_role: nil, requires_actor: nil,
              inject: nil)
      builder = ArgumentListBuilder.new
      yield builder if block_given?

      definition = {
        "name" => name.to_s,
        "return_type" => return_type.to_s,
        "returns_list" => returns_list,
        "nullable" => nullable,
        "arguments" => builder.arguments
      }
      definition["sql_source"] = sql_source.to_s if sql_source
      definition["description"] = description if description
      definition["cache_ttl_seconds"] = cache_ttl_seconds if cache_ttl_seconds
      definition["requires_role"] = requires_role.to_s if requires_role
      definition["requires_actor"] = validated_actors(name, requires_actor) if requires_actor
      definition["inject_params"] = self.class.inject_params(inject) if inject && !inject.empty?

      @queries << definition
      definition
    end

    # Declares a GraphQL mutation.
    #
    # `invalidates_views` and `invalidates_fact_tables` are what connect a write to the
    # cached reads of what it wrote; without them a new row stays invisible for the whole
    # of a reader's TTL.
    def mutation(name, return_type:, sql_source: nil, operation: nil, returns_list: false,
                 nullable: false, description: nil, requires_role: nil, requires_actor: nil,
                 inject: nil, invalidates_views: nil, invalidates_fact_tables: nil)
      builder = ArgumentListBuilder.new
      yield builder if block_given?

      definition = {
        "name" => name.to_s,
        "return_type" => return_type.to_s,
        "returns_list" => returns_list,
        "nullable" => nullable,
        "arguments" => builder.arguments
      }
      definition["sql_source"] = sql_source.to_s if sql_source
      definition["operation"] = operation.to_s if operation
      definition["description"] = description if description
      definition["requires_role"] = requires_role.to_s if requires_role
      definition["requires_actor"] = validated_actors(name, requires_actor) if requires_actor
      definition["inject_params"] = self.class.inject_params(inject) if inject && !inject.empty?
      definition["invalidates_views"] = invalidates_views if invalidates_views
      definition["invalidates_fact_tables"] = invalidates_fact_tables if invalidates_fact_tables

      @mutations << definition
      definition
    end

    # #966's actor allow-list, checked where the author wrote it.
    #
    # The compiler refuses an unknown token by name, but only at compile time, and this is
    # a security gate enforced in the same executor arm as `requires_role` on every
    # transport — one that fails late fails after the author has stopped looking (#1123).
    # An empty list is refused rather than emitted: the compiled schema omits the key when
    # empty, so an empty allow-list reads as a declared gate and compiles to none at all.
    def validated_actors(operation_name, actors)
      actors = Array(actors).map(&:to_s)
      if actors.empty?
        raise ArgumentError,
              "#{operation_name}: requires_actor: is empty. An empty allow-list admits " \
              "nobody and is dropped from the compiled schema, which admits everybody — " \
              "name the actor types instead. Valid: #{ACTOR_TYPES.join(', ')}."
      end

      unknown = actors - ACTOR_TYPES
      unless unknown.empty?
        raise ArgumentError,
              "#{operation_name}: requires_actor: names unknown actor type(s) " \
              "#{unknown.join(', ')}. Valid: #{ACTOR_TYPES.join(', ')}."
      end

      actors
    end

    # The schema as a Hash, in the intermediate format.
    #
    # Empty sections are omitted rather than emitted as `null`: a `null` array is rejected
    # by the compiler with `invalid type: null, expected a sequence` and no key name.
    def to_h
      document = { "version" => "2.0.0", "types" => @types }
      document["enums"] = @enums unless @enums.empty?
      document["queries"] = @queries unless @queries.empty?
      document["mutations"] = @mutations unless @mutations.empty?
      document
    end

    # The schema as a pretty-printed JSON string.
    def to_json(*_args)
      JSON.pretty_generate(to_h)
    end

    # Writes the schema to `path`, ready for `fraiseql compile`.
    def export_json(path)
      File.write(path, "#{to_json}\n")
      path
    end

    # Normalises a `{param => "jwt:claim"}` map into the nested form the compiler reads.
    def self.inject_params(inject)
      inject.each_with_object({}) do |(param, source), out|
        src, claim = source.to_s.split(":", 2)
        if claim.nil? || src.empty? || claim.empty?
          raise ArgumentError,
                "inject_params[#{param.inspect}] must be \"<source>:<claim>\", " \
                "for example \"jwt:tenant_id\", got #{source.inspect}"
        end
        out[param.to_s] = { "source" => src, "claim" => claim }
      end
    end

    # Resolves a Ruby type symbol to its GraphQL type name.
    def self.graphql_type(type)
      return type.to_s if type.is_a?(String)

      SCALARS.fetch(type.to_sym) { type.to_s.split("_").map(&:capitalize).join }
    end

    # Collects the fields declared inside a `type` block.
    class TypeBuilder
      attr_reader :fields, :crud_fields

      def initialize
        @fields = []
        # The same fields in the shape `CrudGenerator` reads, carrying `computed` — which
        # `fields` must not carry. Ruby's generator filters on `f[:computed]`, and the
        # caller it never had built its list without that key, so `reject` would have
        # rejected nothing and every generated input object would have asked the client
        # for server-assigned fields (#1242).
        @crud_fields = []
      end

      # `nullable` is required by the compiler and has no default there; it defaults to
      # true here, matching GraphQL's own default for an unadorned type.
      # `deprecated` accepts `true` for "deprecated, no stated reason" or a String reason,
      # and is emitted as `{ reason: ... }` — the shape `IntermediateField` reads since
      # #1025. There was no parameter here at all, so a Ruby author could not deprecate a
      # field through the path the exporter actually runs.
      def field(name, type, nullable: true, description: nil, requires_scope: nil, on_deny: nil,
                vector_config: nil, vector_distance: nil, deprecated: false, computed: false)
        definition = {
          # camelCase on the way out (#1249). A field is declared as a snake_case symbol
          # because that is the Ruby idiom, and it used to reach the GraphQL API spelled
          # exactly that way — so `t.field :due_date` published `due_date` here while
          # `CrudGenerator`, in this same gem, published `dueDate` in `CreateXInput` for
          # the same column, and the other ten SDKs published `dueDate` too.
          "name" => Naming.snake_to_camel(name),
          "type" => Schema.graphql_type(type),
          "nullable" => nullable
        }
        # `computed` is deliberately absent from `definition`. It is authoring-time only:
        # `CrudGenerator` reads it to decide which fields a client cannot supply, and that
        # runs before export. `IntermediateField` has no `computed` member and denies
        # unknown fields, so emitting it would make the whole document uncompilable — the
        # defect #927 fixed in Python and #1183 found still live in TypeScript and C#.
        @crud_fields << {
          name: definition["name"], type: definition["type"], nullable: nullable, computed: computed
        }
        definition["description"] = description if description
        definition["requires_scope"] = requires_scope.to_s if requires_scope
        definition["on_deny"] = on_deny.to_s if on_deny
        definition["deprecated"] = (deprecated.is_a?(String) ? { "reason" => deprecated } : {}) if deprecated

        add_vector(definition, name, vector_config, vector_distance)

        @fields << definition
        definition
      end

      private

      # A `Vector` field without its config is refused by the compiler, so dropping these
      # would not be a silent loss — it would make the four pgvector field types
      # unauthorable in Ruby.
      #
      # Two refusals here, and only two: a field is either an embedding or the Float
      # reporting how far a search's result was from the query vector, and a column has
      # at least one dimension. Which metrics a field type admits and which index types
      # have an operator class for them depends on pgvector's own tables, and is checked
      # once, in the compiler.
      def add_vector(definition, name, vector_config, vector_distance)
        if vector_config && vector_distance
          raise ArgumentError,
                "Field #{name} declares both vector_config and vector_distance; a field is " \
                "either an embedding or the Float reporting a search's distance, not both"
        end

        definition["vector_config"] = normalize_vector_config(name, vector_config) if vector_config
        # A sibling field's name, so it is spelled the way that sibling is published, or
        # the reference names a field that is no longer in the schema (#1249).
        definition["vector_distance"] = Naming.snake_to_camel(vector_distance) if vector_distance
      end

      # The index type and the metric are written out even where the author left them
      # off, so the emitted schema says which index and which metric the column will get
      # rather than leaving it to a compiler default the author cannot see.
      def normalize_vector_config(name, config)
        dimensions = config[:dimensions] || config["dimensions"]
        unless dimensions.is_a?(Integer) && dimensions >= 1
          raise ArgumentError,
                "Field #{name} declares #{dimensions.inspect} vector dimensions; dimensions " \
                "must be a whole number of at least 1"
        end

        {
          "dimensions" => dimensions,
          "index_type" => (config[:index_type] || config["index_type"] || "hnsw").to_s,
          "distance_metric" => (config[:distance_metric] || config["distance_metric"] || "cosine").to_s
        }
      end
    end

    # Collects the arguments declared inside a `query` or `mutation` block.
    class ArgumentListBuilder
      attr_reader :arguments

      def initialize
        @arguments = []
      end

      def argument(name, type, nullable: true, description: nil)
        definition = {
          "name" => name.to_s,
          "type" => Schema.graphql_type(type),
          "nullable" => nullable
        }
        definition["description"] = description if description

        @arguments << definition
        definition
      end
    end
  end
end
