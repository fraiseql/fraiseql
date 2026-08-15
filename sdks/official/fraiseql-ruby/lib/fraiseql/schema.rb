# frozen_string_literal: true

require "json"

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

    # Index types a pgvector column can be searched through.
    VECTOR_INDEXES = %w[hnsw ivf_flat none].freeze

    # Distance metrics a vector search can order by. Which of them a given field type
    # and index admit is pgvector's business and the compiler's — it holds the
    # operator-class table and refuses a combination that has no class, naming the
    # alternative. This SDK carries no second copy of that table; a copy is what drifts.
    VECTOR_METRICS = %w[cosine l2 inner_product hamming jaccard].freeze

    def initialize
      @types = []
      @input_types = []
      @enums = []
      @queries = []
      @mutations = []
    end

    # Declares a GraphQL object type backed by a SQL view.
    #
    # `is_input: true` declares a GraphQL input object instead. An input object has no
    # backing relation, so `sql_source` is refused on one — the compiler rejects a type
    # that declares both.
    def type(name, sql_source: nil, description: nil, relay: false, is_error: false, is_input: false)
      if is_input && sql_source
        raise ArgumentError,
              "type #{name.inspect}: an input type must not declare sql_source — " \
              "an input object has no backing view."
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
      definition
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
              description: nil, cache_ttl_seconds: nil, requires_role: nil, inject: nil)
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
                 nullable: false, description: nil, requires_role: nil, inject: nil,
                 invalidates_views: nil, invalidates_fact_tables: nil)
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
      definition["inject_params"] = self.class.inject_params(inject) if inject && !inject.empty?
      definition["invalidates_views"] = invalidates_views if invalidates_views
      definition["invalidates_fact_tables"] = invalidates_fact_tables if invalidates_fact_tables

      @mutations << definition
      definition
    end

    # The schema as a Hash, in the intermediate format.
    #
    # Empty sections are omitted rather than emitted as `null`: a `null` array is rejected
    # by the compiler with `invalid type: null, expected a sequence` and no key name.
    def to_h
      document = { "version" => "2.0.0", "types" => @types }
      document["enums"] = @enums unless @enums.empty?
      document["input_types"] = @input_types unless @input_types.empty?
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
      attr_reader :fields

      def initialize
        @fields = []
      end

      # `nullable` is required by the compiler and has no default there; it defaults to
      # true here, matching GraphQL's own default for an unadorned type.
      def field(name, type, nullable: true, description: nil, requires_scope: nil, on_deny: nil,
                vector_config: nil, vector_distance: nil)
        definition = {
          "name" => name.to_s,
          "type" => Schema.graphql_type(type),
          "nullable" => nullable
        }
        definition["description"] = description if description
        definition["requires_scope"] = requires_scope.to_s if requires_scope
        definition["on_deny"] = on_deny.to_s if on_deny

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
        definition["vector_distance"] = vector_distance.to_s if vector_distance
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
