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

  context "with 403 response" do
    let(:body) { "" }

    before do
      stub_request(:post, "#{url}/graphql").to_return(status: 403, body: "")
    end

    it "raises AuthenticationError with status_code 403" do
      expect { client.query("{ admin }") }.to raise_error(FraiseQL::AuthenticationError) do |e|
        expect(e.status_code).to eq(403)
      end
    end
  end

  context "with 500 response and JSON error body" do
    let(:body) { "" }

    before do
      stub_request(:post, "#{url}/graphql").to_return(
        status: 500,
        headers: { "Content-Type" => "application/json" },
        body: '{"errors": [{"message": "Internal server error"}]}'
      )
    end

    it "raises GraphQLError (parsed from JSON body)" do
      expect { client.query("{ data }") }.to raise_error(FraiseQL::GraphQLError) do |e|
        expect(e.message).to eq("Internal server error")
      end
    end
  end

  context "with operationName provided" do
    let(:body) { '{"data": {"user": {"id": 1}}}' }

    it "sends operationName in the request body" do
      client.query("query GetUser { user { id } }", operation_name: "GetUser")
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with { |req| JSON.parse(req.body)["operationName"] == "GetUser" }
    end
  end

  context "with operationName not provided" do
    let(:body) { '{"data": {"user": {"id": 1}}}' }

    it "does not send operationName key in the request body" do
      client.query("{ user { id } }")
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with { |req| !JSON.parse(req.body).key?("operationName") }
    end
  end

  context "with empty errors array (cross-SDK invariant)" do
    let(:body) { '{"data": {"users": []}, "errors": []}' }

    it "does not raise an error" do
      expect { client.query("{ users { id } }") }.not_to raise_error
    end

    it "returns the data hash" do
      result = client.query("{ users { id } }")
      expect(result).to eq("users" => [])
    end
  end

  context "with empty data" do
    let(:body) { '{"data": {}}' }

    it "returns an empty hash" do
      result = client.query("{ nothing }")
      expect(result).to eq({})
    end
  end

  context "without authorization" do
    let(:body) { '{"data": {}}' }

    it "does not send an Authorization header" do
      client.query("{ public }")
      expect(WebMock).not_to have_requested(:post, "#{url}/graphql")
        .with(headers: { "Authorization" => /.*/ })
    end
  end

  context "URL path normalization" do
    let(:body) { '{"data": {}}' }

    it "appends /graphql when URL has no path" do
      stub_request(:post, "http://localhost:4000/graphql")
        .to_return(headers: { "Content-Type" => "application/json" }, body: body)
      c = described_class.new("http://localhost:4000")
      c.query("{ ok }")
      expect(WebMock).to have_requested(:post, "http://localhost:4000/graphql")
    end

    it "replaces root / path with /graphql" do
      stub_request(:post, "http://localhost:4000/graphql")
        .to_return(headers: { "Content-Type" => "application/json" }, body: body)
      c = described_class.new("http://localhost:4000/")
      c.query("{ ok }")
      expect(WebMock).to have_requested(:post, "http://localhost:4000/graphql")
    end

    it "keeps /graphql path unchanged" do
      stub_request(:post, "http://localhost:4000/graphql")
        .to_return(headers: { "Content-Type" => "application/json" }, body: body)
      c = described_class.new("http://localhost:4000/graphql")
      c.query("{ ok }")
      expect(WebMock).to have_requested(:post, "http://localhost:4000/graphql")
    end
  end

  context "request headers" do
    let(:body) { '{"data": {}}' }

    it "always sends Content-Type: application/json" do
      client.query("{ ok }")
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { "Content-Type" => "application/json" })
    end

    it "always sends Accept: application/json" do
      client.query("{ ok }")
      expect(WebMock).to have_requested(:post, "#{url}/graphql")
        .with(headers: { "Accept" => "application/json" })
    end
  end
end
