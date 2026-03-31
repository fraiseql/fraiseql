# frozen_string_literal: true

require 'spec_helper'

RSpec.describe FraiseQL::Error do
  it 'is a StandardError' do
    expect(described_class.ancestors).to include(StandardError)
  end

  describe FraiseQL::GraphQLError do
    it 'is a FraiseQL::Error' do
      expect(described_class.ancestors).to include(FraiseQL::Error)
    end

    describe '#initialize' do
      it 'stores the errors array' do
        errors = [{ 'message' => 'Not found' }, { 'message' => 'Forbidden' }]
        error = described_class.new(errors)
        expect(error.errors).to eq(errors)
      end

      it "sets message to the first error's message (string keys)" do
        error = described_class.new([{ 'message' => 'First error' }, { 'message' => 'Second error' }])
        expect(error.message).to eq('First error')
      end

      it "sets message to the first error's message (symbol keys)" do
        error = described_class.new([{ message: 'Symbol key error' }])
        expect(error.message).to eq('Symbol key error')
      end

      it "falls back to 'GraphQL error' when no message key is present" do
        error = described_class.new([{}])
        expect(error.message).to eq('GraphQL error')
      end

      it "falls back to 'GraphQL error' when errors array contains only empty hashes" do
        error = described_class.new([{}, {}])
        expect(error.message).to eq('GraphQL error')
      end
    end
  end

  describe FraiseQL::NetworkError do
    it 'is a FraiseQL::Error' do
      expect(described_class.ancestors).to include(FraiseQL::Error)
    end
  end

  describe FraiseQL::TimeoutError do
    it 'is a FraiseQL::NetworkError' do
      expect(described_class.ancestors).to include(FraiseQL::NetworkError)
    end

    it 'is also a FraiseQL::Error' do
      expect(described_class.ancestors).to include(FraiseQL::Error)
    end

    it 'is a NetworkError (can be rescued as NetworkError)' do
      error = described_class.new('read timeout')
      expect(error).to be_a(FraiseQL::NetworkError)
    end

    it 'stores the message' do
      error = described_class.new('connection timed out')
      expect(error.message).to eq('connection timed out')
    end
  end

  describe FraiseQL::AuthenticationError do
    it 'is a FraiseQL::Error' do
      expect(described_class.ancestors).to include(FraiseQL::Error)
    end

    describe '#initialize' do
      it 'stores status_code 401' do
        error = described_class.new(401)
        expect(error.status_code).to eq(401)
      end

      it 'stores status_code 403' do
        error = described_class.new(403)
        expect(error.status_code).to eq(403)
      end

      it 'includes HTTP status code in the message' do
        expect(described_class.new(401).message).to eq('Authentication failed (HTTP 401)')
        expect(described_class.new(403).message).to eq('Authentication failed (HTTP 403)')
      end
    end
  end

  describe FraiseQL::RateLimitError do
    it 'is a FraiseQL::Error' do
      expect(described_class.ancestors).to include(FraiseQL::Error)
    end

    describe '#initialize' do
      it 'sets retry_after to nil by default' do
        error = described_class.new
        expect(error.retry_after).to be_nil
      end

      it 'stores a numeric retry_after' do
        error = described_class.new(retry_after: 120)
        expect(error.retry_after).to eq(120)
      end

      it 'stores a string retry_after (e.g. HTTP date header)' do
        error = described_class.new(retry_after: 'Wed, 21 Oct 2015 07:28:00 GMT')
        expect(error.retry_after).to eq('Wed, 21 Oct 2015 07:28:00 GMT')
      end

      it "has the default message 'Rate limit exceeded'" do
        expect(described_class.new.message).to eq('Rate limit exceeded')
      end
    end
  end

  it 'makes all error classes rescuable as FraiseQL::Error' do
    errors = [
      FraiseQL::GraphQLError.new([{ 'message' => 'x' }]),
      FraiseQL::NetworkError.new('x'),
      FraiseQL::TimeoutError.new('x'),
      FraiseQL::AuthenticationError.new(401),
      FraiseQL::RateLimitError.new
    ]
    expect(errors).to all be_a(described_class)
  end
end
