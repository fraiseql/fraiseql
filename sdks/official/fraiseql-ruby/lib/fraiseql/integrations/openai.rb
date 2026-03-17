# frozen_string_literal: true

require "json"

module FraiseQL
  module OpenAI
    class Tool
      def initialize(client, name:, description:, query:, parameters_schema:)
        @client = client
        @name = name
        @description = description
        @query = query
        @parameters_schema = parameters_schema
      end

      def to_definition
        {
          type: "function",
          function: {
            name: @name,
            description: @description,
            parameters: @parameters_schema
          }
        }
      end

      def call(arguments)
        result = @client.query(@query, variables: arguments)
        JSON.generate(result)
      end
    end
  end
end
