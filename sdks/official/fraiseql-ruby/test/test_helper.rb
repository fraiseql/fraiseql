# frozen_string_literal: true

require "minitest/autorun"

$LOAD_PATH.unshift(File.expand_path("../lib", __dir__))

# The gem entry point, which the README's Quick Start opens with. These tests used
# to require each file individually — which is why nobody noticed that
# `require "fraiseql"` raised LoadError (#853).
require "fraiseql"
require "fraiseql/version"
require "fraiseql/errors"
require "fraiseql/retry"
require "fraiseql/authoring/crud_generator"
require "fraiseql/authoring/type"
require "fraiseql/client"
