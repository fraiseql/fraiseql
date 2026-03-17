# frozen_string_literal: true

module FraiseQL
  class Error < StandardError; end

  class GraphQLError < Error
    attr_reader :errors

    def initialize(errors)
      @errors = errors
      super(errors.first&.dig("message") || errors.first&.dig(:message) || "GraphQL error")
    end
  end

  class NetworkError < Error; end
  class TimeoutError < NetworkError; end

  class AuthenticationError < Error
    attr_reader :status_code

    def initialize(status_code)
      @status_code = status_code
      super("Authentication failed (HTTP #{status_code})")
    end
  end

  class RateLimitError < Error
    attr_reader :retry_after

    def initialize(retry_after: nil)
      @retry_after = retry_after
      super("Rate limit exceeded")
    end
  end
end
