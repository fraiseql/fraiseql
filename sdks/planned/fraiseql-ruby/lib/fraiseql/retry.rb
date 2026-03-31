# frozen_string_literal: true

module FraiseQL
  # Configuration for automatic request retries with exponential backoff.
  class RetryConfig
    attr_reader :max_attempts, :base_delay, :max_delay, :jitter, :retry_on

    # @param max_attempts [Integer] total attempts (including the first)
    # @param base_delay [Numeric] initial delay in seconds
    # @param max_delay [Numeric] maximum delay cap in seconds
    # @param jitter [Boolean] whether to add random jitter to delays
    # @param retry_on [Array<Class>] error classes that are retryable
    def initialize(max_attempts: 3, base_delay: 1.0, max_delay: 30.0, jitter: true, retry_on: [NetworkError])
      @max_attempts = max_attempts
      @base_delay = base_delay
      @max_delay = max_delay
      @jitter = jitter
      @retry_on = retry_on
    end

    # Whether the given error should trigger a retry.
    #
    # @param error [Exception]
    # @return [Boolean]
    def retryable?(error)
      @retry_on.any? { |klass| error.is_a?(klass) }
    end

    # Compute the delay for a given attempt number (0-indexed).
    # Uses exponential backoff: base_delay * 2^attempt, capped at max_delay.
    #
    # @param attempt [Integer] zero-based attempt index
    # @return [Float] delay in seconds
    def delay_for(attempt)
      delay = @base_delay * (2**attempt)
      delay = [@max_delay, delay].min
      delay *= rand(0.5..1.0) if @jitter
      delay
    end
  end
end
