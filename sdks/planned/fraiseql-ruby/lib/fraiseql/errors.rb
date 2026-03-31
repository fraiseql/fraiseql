# frozen_string_literal: true

module FraiseQL
  # Base error class for all FraiseQL errors.
  class Error < StandardError; end

  # Raised when the GraphQL response contains an errors array.
  class GraphQLError < Error
    attr_reader :errors

    # @param errors [Array<Hash>] the raw errors array from the response
    def initialize(errors)
      @errors = errors
      first_message = errors.first&.fetch('message', nil) ||
                      errors.first&.fetch(:message, nil) ||
                      'GraphQL error'
      super(first_message)
    end
  end

  # Raised on transport-level failures (connection refused, DNS, etc.).
  class NetworkError < Error; end

  # Raised on request timeouts. A subclass of NetworkError so callers
  # can rescue either specifically or broadly.
  class TimeoutError < NetworkError; end

  # Raised when the server responds with 401 or 403.
  class AuthenticationError < Error
    attr_reader :status_code

    # @param status_code [Integer] the HTTP status code (401 or 403)
    def initialize(status_code)
      @status_code = status_code
      super("Authentication failed (HTTP #{status_code})")
    end
  end

  # Raised when the server responds with 429.
  class RateLimitError < Error
    attr_reader :retry_after

    # @param retry_after [Integer, String, nil] value of the Retry-After header
    def initialize(retry_after: nil)
      @retry_after = retry_after
      super('Rate limit exceeded')
    end
  end
end
