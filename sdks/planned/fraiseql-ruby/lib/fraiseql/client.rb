# frozen_string_literal: true

require 'json'
require 'net/http'
require 'uri'

module FraiseQL
  # HTTP client for executing GraphQL queries against a FraiseQL server.
  class Client
    # @param url [String] base URL of the FraiseQL server
    # @param authorization [String, nil] value for the Authorization header
    # @param retry_config [RetryConfig, nil] retry configuration
    def initialize(url, authorization: nil, retry_config: nil)
      @uri = build_uri(url)
      @authorization = authorization
      @retry_config = retry_config
    end

    # Execute a GraphQL query.
    #
    # @param query_string [String] the GraphQL query
    # @param variables [Hash, nil] query variables
    # @param operation_name [String, nil] the operation name
    # @return [Hash] the "data" portion of the response
    # @raise [GraphQLError, AuthenticationError, RateLimitError, NetworkError]
    def query(query_string, variables: nil, operation_name: nil)
      execute(query_string, variables: variables, operation_name: operation_name)
    end

    # Execute a GraphQL mutation (alias for query).
    #
    # @param query_string [String] the GraphQL mutation
    # @param variables [Hash, nil] mutation variables
    # @param operation_name [String, nil] the operation name
    # @return [Hash] the "data" portion of the response
    # @raise [GraphQLError, AuthenticationError, RateLimitError, NetworkError]
    def mutate(query_string, variables: nil, operation_name: nil)
      execute(query_string, variables: variables, operation_name: operation_name)
    end

    private

    def build_uri(url)
      uri = URI.parse(url)
      uri.path = '/graphql' if uri.path.nil? || uri.path.empty? || uri.path == '/'
      uri
    end

    def execute(query_string, variables: nil, operation_name: nil)
      body = { 'query' => query_string }
      body['variables'] = variables if variables
      body['operationName'] = operation_name if operation_name

      attempt = 0
      max = @retry_config&.max_attempts || 1

      begin
        attempt += 1
        response = perform_request(body)
        handle_response(response)
      rescue FraiseQL::Error => e
        if @retry_config && attempt < max && @retry_config.retryable?(e)
          sleep(@retry_config.delay_for(attempt - 1))
          retry
        end
        raise
      end
    end

    def perform_request(body)
      http = Net::HTTP.new(@uri.host, @uri.port)
      http.use_ssl = @uri.scheme == 'https'

      request = Net::HTTP::Post.new(@uri.request_uri)
      request['Content-Type'] = 'application/json'
      request['Accept'] = 'application/json'
      request['Authorization'] = @authorization if @authorization

      request.body = JSON.generate(body)
      http.request(request)
    rescue Errno::ECONNREFUSED, Errno::ECONNRESET, Errno::EHOSTUNREACH,
           SocketError, IOError => e
      raise NetworkError, e.message
    rescue Net::ReadTimeout, Net::OpenTimeout => e
      raise TimeoutError, e.message
    end

    def handle_response(response)
      status = response.code.to_i

      raise AuthenticationError, status if [401, 403].include?(status)
      raise RateLimitError.new(retry_after: response['Retry-After']) if status == 429

      parsed = begin
        JSON.parse(response.body)
      rescue JSON::ParserError, TypeError
        raise NetworkError, "Unexpected response (HTTP #{status})"
      end

      errors = parsed['errors']
      raise GraphQLError, errors if errors.is_a?(Array) && !errors.empty?

      parsed.fetch('data', {})
    end
  end
end
