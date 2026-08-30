# frozen_string_literal: true

module FraiseQL
  # The one place a declared identifier becomes the name the GraphQL API publishes.
  #
  # Ruby declares fields, arguments and operations as snake_case symbols because that is
  # the Ruby idiom. The engine's convention is camelCase (`NamingConvention::CamelCase`),
  # and the compiler does not translate for you — whatever an SDK emits is what clients
  # see. So the translation has to happen here, once, on the way out (#1249).
  #
  # Before this module there were two answers in one gem: `CrudGenerator` camelCased the
  # operations and input-object fields it generated, while `Schema::TypeBuilder` and
  # `Type#to_fraiseql_schema` emitted the symbol verbatim. One `crud` declaration
  # therefore published `due_date` on the type and `dueDate` in `CreateXInput`, for the
  # same column.
  module Naming
    module_function

    # Convert a snake_case name to camelCase.
    #
    # This is the engine's `to_camel_case` (`fraiseql-core/src/utils/casing.rs`) rule
    # exactly: drop each underscore and upcase the single character after it, leaving
    # every other character alone. A name with no underscore is already camelCase and is
    # returned unchanged, which makes this idempotent.
    #
    # Two properties are easy to get wrong and both were:
    #
    # - a digit segment collapses onto the previous word — `phone_1` is `phone1` and
    #   `dns_1_id` is `dns1Id`. The previous implementation was `/_([a-z])/`, whose
    #   character class does not match a digit, so it left `phone_1` untouched while the
    #   engine and the Python SDK produced `phone1`;
    # - the rest of a segment keeps its case, so `user_ID` is `userID`, not `userId`.
    #
    # @param name [String, Symbol] the snake_case name
    # @return [String] the camelCase equivalent
    def snake_to_camel(name)
      s = name.to_s
      return s unless s.include?("_")

      out = +""
      upcase_next = false
      s.each_char do |c|
        if c == "_"
          upcase_next = true
        elsif upcase_next
          out << c.upcase
          upcase_next = false
        else
          out << c
        end
      end
      out
    end
  end
end
