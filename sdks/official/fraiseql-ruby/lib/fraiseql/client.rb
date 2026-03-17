# frozen_string_literal: true

require "net/http"
require "uri"
require "json"

module FraiseQL
  class Client
    def initialize(url, authorization: nil, timeout: 30, retry_config: nil)
      @uri = URI.parse(url)
      @uri.path = "/graphql" if @uri.path.empty? || @uri.path == "/"
      @authorization = authorization
      @timeout = timeout
      @retry_config = retry_config
    end

    # @param query [String] GraphQL query
    # @param variables [Hash, nil] query variables
    # @param operation_name [String, nil] optional operation name
    # @return [Hash] parsed data from the response
    # @raise [FraiseQL::GraphQLError] if errors are present and non-null
    # @raise [FraiseQL::NetworkError] on transport failure
    def query(query, variables: nil, operation_name: nil)
      payload = { query: query, variables: variables }
      payload[:operationName] = operation_name if operation_name
      execute(payload)
    end

    # @param mutation [String]
    # @param variables [Hash, nil]
    # @param operation_name [String, nil] optional operation name
    # @return [Hash] parsed data from the response
    def mutate(mutation, variables: nil, operation_name: nil)
      payload = { query: mutation, variables: variables }
      payload[:operationName] = operation_name if operation_name
      execute(payload)
    end

    private

    def execute(body)
      max_attempts = @retry_config&.max_attempts || 1
      attempt = 0
      last_error = nil

      while attempt < max_attempts
        begin
          return do_request(body)
        rescue FraiseQL::Error => e
          last_error = e
          attempt += 1
          break unless @retry_config&.retryable?(e) && attempt < max_attempts

          sleep(@retry_config.delay_for(attempt - 1))
        end
      end

      raise last_error if last_error
    end

    def do_request(body)
      http = Net::HTTP.new(@uri.host, @uri.port)
      http.use_ssl = @uri.scheme == "https"
      http.open_timeout = @timeout
      http.read_timeout = @timeout

      request = Net::HTTP::Post.new(@uri.request_uri)
      request["Content-Type"] = "application/json"
      request["Accept"] = "application/json"
      request["Authorization"] = @authorization if @authorization
      request.body = JSON.generate(body.compact)

      response = http.request(request)
      parse_response(response)
    rescue Net::OpenTimeout, Net::ReadTimeout => e
      raise TimeoutError, e.message
    rescue Errno::ECONNREFUSED, SocketError, Errno::EHOSTUNREACH => e
      raise NetworkError, e.message
    end

    def parse_response(response)
      case response.code.to_i
      when 401, 403 then raise AuthenticationError.new(response.code.to_i)
      when 429 then raise RateLimitError.new
      end

      parsed = JSON.parse(response.body)
      errors = parsed["errors"]
      # null errors = success (cross-SDK invariant: nil and empty array both succeed)
      raise GraphQLError.new(errors) if errors.is_a?(Array) && !errors.empty?

      parsed["data"] || {}
    end
  end
end
