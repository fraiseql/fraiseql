# frozen_string_literal: true

require_relative 'fraiseql/version'
require_relative 'fraiseql/errors'
require_relative 'fraiseql/retry'
require_relative 'fraiseql/client'
require_relative 'fraiseql/authoring/type'

module FraiseQL
  # Autoload integrations
  autoload :OpenAI, 'fraiseql/integrations/openai'
end
