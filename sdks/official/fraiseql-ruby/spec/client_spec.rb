# frozen_string_literal: true

require "spec_helper"

RSpec.describe FraiseQL::Client do
  let(:url) { "http://localhost:8000" }
  let(:client) { described_class.new(url) }

  before do
    stub_request(:post, "#{url}/graphql").to_return(
      headers: { "Content-Type" => "application/json" },
      body: body
    )
  end

  context "with successful response" do
    let(:body) { '{"data": {"user": {"id": 1, "name": "Alice"}}}' }

    it "returns the data hash" do
      result = client.query("{ user { id name } }")
      expect(result).to eq("user" => { "id" => 1, "name" => "Alice" })
    end
  end

  context "with errors array" do
    let(:body) { '{"data": null, "errors": [{"message": "Not found"}]}' }

    it "raises GraphQLError" do
      expect { client.query("{ user { id } }") }.to raise_error(FraiseQL::GraphQLError)
    end
  end

  context "with null errors (regression - cross-SDK invariant)" do
    let(:body) { '{"data": {"users": []}, "errors": null}' }

    it "does not raise" do
      expect { client.query("{ users { id } }") }.not_to raise_error
    end
  end

  context "with 401 response" do
    let(:body) { "" }

    before do
      stub_request(:post, "#{url}/graphql").to_return(status: 401, body: "")
    end

    it "raises AuthenticationError" do
      expect { client.query("{ secret }") }.to raise_error(FraiseQL::AuthenticationError)
    end
  end

  context "with 429 response" do
    let(:body) { "" }

    before do
      stub_request(:post, "#{url}/graphql").to_return(status: 429, body: "")
    end

    it "raises RateLimitError" do
      expect { client.query("{ data }") }.to raise_error(FraiseQL::RateLimitError)
    end
  end

  context "with variables" do
    let(:body) { '{"data": {"user": {"id": 42}}}' }

    it "sends variables in request body" do
      client.query("query($id: ID!) { user(id: $id) { id } }", variables: { id: "42" })
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with { |req| JSON.parse(req.body)["variables"] == { "id" => "42" } }
    end
  end

  context "mutate" do
    let(:body) { '{"data": {"createUser": {"id": 1}}}' }

    it "returns the data hash" do
      result = client.mutate("mutation { createUser { id } }")
      expect(result).to eq("createUser" => { "id" => 1 })
    end
  end

  context "with authorization header" do
    let(:body) { '{"data": {}}' }
    let(:client) { described_class.new(url, authorization: "Bearer token123") }

    it "sends Authorization header" do
      client.query("{ me { id } }")
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { "Authorization" => "Bearer token123" })
    end
  end
end
