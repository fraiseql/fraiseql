# frozen_string_literal: true

module FraiseQL
  class RetryConfig
    attr_reader :max_attempts, :base_delay, :max_delay, :jitter, :retry_on

    def initialize(
      max_attempts: 1,
      base_delay: 1.0,
      max_delay: 30.0,
      jitter: true,
      retry_on: [NetworkError, TimeoutError]
    )
      @max_attempts = max_attempts
      @base_delay = base_delay
      @max_delay = max_delay
      @jitter = jitter
      @retry_on = retry_on
    end

    def delay_for(attempt)
      delay = [@base_delay * (2**attempt), @max_delay].min
      jitter ? delay + rand * delay * 0.1 : delay
    end

    def retryable?(error)
      @retry_on.any? { |klass| error.is_a?(klass) }
    end
  end
end
