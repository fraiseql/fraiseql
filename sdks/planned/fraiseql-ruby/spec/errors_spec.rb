# frozen_string_literal: true

require 'spec_helper'

RSpec.describe FraiseQL::GraphQLError do
  it 'extracts the first error message' do
    error = described_class.new([{ 'message' => 'Not found' }])
    expect(error.message).to eq('Not found')
  end

  it 'stores the errors array' do
    errors = [{ 'message' => 'Forbidden' }, { 'message' => 'Validation failed' }]
    error = described_class.new(errors)
    expect(error.errors).to eq(errors)
  end

  it 'handles symbol keys' do
    error = described_class.new([{ message: 'Symbol key error' }])
    expect(error.message).to eq('Symbol key error')
  end

  it 'uses a default message when errors is empty' do
    error = described_class.new([{}])
    expect(error.message).to eq('GraphQL error')
  end

  describe FraiseQL::AuthenticationError do
    it 'includes the HTTP status in the message' do
      error = described_class.new(401)
      expect(error.message).to eq('Authentication failed (HTTP 401)')
      expect(error.status_code).to eq(401)
    end
  end

  describe FraiseQL::RateLimitError do
    it 'has a default message' do
      error = described_class.new
      expect(error.message).to eq('Rate limit exceeded')
      expect(error.retry_after).to be_nil
    end

    it 'stores retry_after when provided' do
      error = described_class.new(retry_after: 60)
      expect(error.retry_after).to eq(60)
    end
  end
end
