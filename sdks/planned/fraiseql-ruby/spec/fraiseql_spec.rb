# frozen_string_literal: true

require "spec_helper"

RSpec.describe FraiseQL do
  it "has a version constant" do
    expect(FraiseQL::VERSION).to eq("2.2.0")
  end
end
