# frozen_string_literal: true

require 'spec_helper'

RSpec.describe FraiseQL::RetryConfig do
  describe '#retryable?' do
    it 'returns true for NetworkError by default' do
      config = described_class.new
      expect(config.retryable?(FraiseQL::NetworkError.new('connection refused'))).to be true
    end

    it 'returns true for TimeoutError by default (subclass of NetworkError)' do
      config = described_class.new
      expect(config.retryable?(FraiseQL::TimeoutError.new('timed out'))).to be true
    end

    it 'returns false for GraphQLError by default' do
      config = described_class.new
      expect(config.retryable?(FraiseQL::GraphQLError.new([{ 'message' => 'Not found' }]))).to be false
    end

    it 'returns false for AuthenticationError by default' do
      config = described_class.new
      expect(config.retryable?(FraiseQL::AuthenticationError.new(401))).to be false
    end

    it 'returns false for RateLimitError by default' do
      config = described_class.new
      expect(config.retryable?(FraiseQL::RateLimitError.new)).to be false
    end

    it 'respects custom retry_on list' do
      config = described_class.new(retry_on: [FraiseQL::RateLimitError])
      expect(config.retryable?(FraiseQL::RateLimitError.new)).to be true
      expect(config.retryable?(FraiseQL::NetworkError.new('x'))).to be false
    end
  end

  describe '#delay_for' do
    it 'returns base_delay for the first attempt' do
      config = described_class.new(base_delay: 1.0, jitter: false)
      expect(config.delay_for(0)).to eq(1.0)
    end

    it 'doubles delay with each attempt (exponential backoff)' do
      config = described_class.new(base_delay: 1.0, jitter: false)
      expect(config.delay_for(1)).to eq(2.0)
      expect(config.delay_for(2)).to eq(4.0)
    end

    it 'caps delay at max_delay' do
      config = described_class.new(base_delay: 1.0, max_delay: 5.0, jitter: false)
      expect(config.delay_for(10)).to eq(5.0)
    end

    it 'adds jitter when jitter is true' do
      config = described_class.new(base_delay: 1.0, jitter: true)
      delays = Array.new(20) { config.delay_for(0) }
      expect(delays.uniq.size).to be > 1
    end

    it 'does not add jitter when jitter is false' do
      config = described_class.new(base_delay: 1.0, jitter: false)
      delays = Array.new(5) { config.delay_for(0) }
      expect(delays.uniq.size).to eq(1)
    end
  end

  describe FraiseQL::Client do
    let(:url) { 'http://localhost:8000' }

    context 'with max_attempts=1 (default, no retry config)' do
      it 'does not retry on network error' do
        stub_request(:post, "#{url}/graphql").to_raise(Errno::ECONNREFUSED)
        client = described_class.new(url)
        expect { client.query('{ ok }') }.to raise_error(FraiseQL::NetworkError)
        expect(WebMock).to have_requested(:post, "#{url}/graphql").once
      end
    end

    context 'with retry_config max_attempts=1' do
      it 'does not retry when max_attempts is 1' do
        stub_request(:post, "#{url}/graphql").to_raise(Errno::ECONNREFUSED)
        config = FraiseQL::RetryConfig.new(max_attempts: 1, base_delay: 0, jitter: false)
        client = described_class.new(url, retry_config: config)
        expect { client.query('{ ok }') }.to raise_error(FraiseQL::NetworkError)
        expect(WebMock).to have_requested(:post, "#{url}/graphql").once
      end
    end

    context 'with retry_config max_attempts=3 on retryable error' do
      let(:retry_config) do
        FraiseQL::RetryConfig.new(
          max_attempts: 3,
          base_delay: 0,
          jitter: false,
          retry_on: [FraiseQL::NetworkError]
        )
      end

      it 'retries up to max_attempts and raises last error' do
        stub_request(:post, "#{url}/graphql").to_raise(Errno::ECONNREFUSED)
        client = described_class.new(url, retry_config: retry_config)
        expect { client.query('{ ok }') }.to raise_error(FraiseQL::NetworkError)
        expect(WebMock).to have_requested(:post, "#{url}/graphql").times(3)
      end

      it 'succeeds on a later attempt' do
        call_count = 0
        stub_request(:post, "#{url}/graphql").to_return do |_request|
          call_count += 1
          raise Errno::ECONNREFUSED if call_count < 3

          {
            status: 200,
            headers: { 'Content-Type' => 'application/json' },
            body: '{"data": {"ok": true}}'
          }
        end
        client = described_class.new(url, retry_config: retry_config)
        result = client.query('{ ok }')
        expect(result).to eq('ok' => true)
        expect(WebMock).to have_requested(:post, "#{url}/graphql").times(3)
      end
    end

    context 'with retry_config on non-retryable error' do
      let(:retry_config) do
        FraiseQL::RetryConfig.new(
          max_attempts: 3,
          base_delay: 0,
          jitter: false,
          retry_on: [FraiseQL::NetworkError]
        )
      end

      it 'does not retry on GraphQLError' do
        stub_request(:post, "#{url}/graphql").to_return(
          status: 200,
          headers: { 'Content-Type' => 'application/json' },
          body: '{"errors": [{"message": "Not found"}]}'
        )
        client = described_class.new(url, retry_config: retry_config)
        expect { client.query('{ ok }') }.to raise_error(FraiseQL::GraphQLError)
        expect(WebMock).to have_requested(:post, "#{url}/graphql").once
      end

      it 'does not retry on AuthenticationError' do
        stub_request(:post, "#{url}/graphql").to_return(status: 401, body: '')
        client = described_class.new(url, retry_config: retry_config)
        expect { client.query('{ ok }') }.to raise_error(FraiseQL::AuthenticationError)
        expect(WebMock).to have_requested(:post, "#{url}/graphql").once
      end
    end

    context 'when exhausting all retry attempts' do
      it 'raises the last error after all attempts are used' do
        stub_request(:post, "#{url}/graphql").to_raise(Errno::ECONNREFUSED)
        config = FraiseQL::RetryConfig.new(
          max_attempts: 2,
          base_delay: 0,
          jitter: false,
          retry_on: [FraiseQL::NetworkError]
        )
        client = described_class.new(url, retry_config: config)
        error = nil
        begin
          client.query('{ ok }')
        rescue FraiseQL::NetworkError => e
          error = e
        end
        expect(error).to be_a(FraiseQL::NetworkError)
      end
    end
  end
end
