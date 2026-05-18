# frozen_string_literal: true

require "minitest/autorun"

$LOAD_PATH.unshift(File.expand_path("../lib", __dir__))

require "fraiseql/version"
require "fraiseql/errors"
require "fraiseql/retry"
require "fraiseql/authoring/crud_generator"
require "fraiseql/authoring/type"
require "fraiseql/client"
