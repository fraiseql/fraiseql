# frozen_string_literal: true

require 'spec_helper'

RSpec.describe FraiseQL::Client do
  let(:url) { 'http://localhost:8000' }
  let(:success_body) { '{"data": {"ok": true}}' }

  before do
    stub_request(:post, "#{url}/graphql")
      .to_return(
        status: 200,
        headers: { 'Content-Type' => 'application/json' },
        body: success_body
      )
  end

  context 'with Content-Type header' do
    it 'is always application/json for query' do
      FraiseQL::Client.new(url).query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Content-Type' => 'application/json' })
    end

    it 'is always application/json for mutate' do
      FraiseQL::Client.new(url).mutate('mutation { doIt }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Content-Type' => 'application/json' })
    end

    it 'is always application/json even when Authorization is present' do
      FraiseQL::Client.new(url, authorization: 'Bearer tok').query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Content-Type' => 'application/json' })
    end
  end

  context 'with Accept header' do
    it 'is always application/json for query' do
      FraiseQL::Client.new(url).query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Accept' => 'application/json' })
    end

    it 'is always application/json for mutate' do
      FraiseQL::Client.new(url).mutate('mutation { doIt }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Accept' => 'application/json' })
    end
  end

  context 'with Authorization header' do
    it 'is present when authorization string is provided' do
      FraiseQL::Client.new(url, authorization: 'Bearer mytoken').query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Authorization' => 'Bearer mytoken' })
    end

    it 'is absent when no authorization is provided' do
      FraiseQL::Client.new(url).query('{ ok }')
      expect(WebMock).not_to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Authorization' => /.*/ })
    end

    it 'sends the authorization string verbatim (Bearer scheme)' do
      FraiseQL::Client.new(url, authorization: 'Bearer abc.def.ghi').query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Authorization' => 'Bearer abc.def.ghi' })
    end

    it 'sends the authorization string verbatim (API key scheme)' do
      FraiseQL::Client.new(url, authorization: 'ApiKey secret-key-123').query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Authorization' => 'ApiKey secret-key-123' })
    end

    it 'sends the authorization string verbatim (Basic scheme)' do
      FraiseQL::Client.new(url, authorization: 'Basic dXNlcjpwYXNz').query('{ ok }')
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { 'Authorization' => 'Basic dXNlcjpwYXNz' })
    end
  end
end
