# frozen_string_literal: true

# FraiseQL Ruby SDK — schema authoring and HTTP client.
#
# `require "fraiseql"` is the first line of the README's Quick Start and it used to raise
# `LoadError`: the gemspec sets `require_paths = ["lib"]` and there was no `lib/fraiseql.rb`
# at all, only the `lib/fraiseql/` directory. The package's own tests never hit it because
# they require the individual files directly (#853).
require_relative "fraiseql/version"
require_relative "fraiseql/naming"
require_relative "fraiseql/errors"
require_relative "fraiseql/retry"
require_relative "fraiseql/client"
require_relative "fraiseql/schema"
require_relative "fraiseql/authoring/type"
require_relative "fraiseql/authoring/crud_generator"

module FraiseQL
end
