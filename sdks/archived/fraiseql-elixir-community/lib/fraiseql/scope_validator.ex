defmodule FraiseQL.ScopeValidator do
  @moduledoc """
  Validator for field-level scope format and patterns

  Scope format: action:resource
  Examples: read:user.email, admin:*, write:Post.*

  Rules:
  - Action: [a-zA-Z_][a-zA-Z0-9_]*
  - Resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
  - Global wildcard: *
  """

  def validate(scope) when is_binary(scope) do
    cond do
      scope == "" -> {:error, "Scope cannot be empty"}
      scope == "*" -> :ok
      true -> validate_format(scope)
    end
  end

  defp validate_format(scope) do
    case String.split(scope, ":", parts: 2) do
      [action, resource] ->
        case validate_action(action) do
          :ok ->
            case validate_resource(resource) do
              :ok -> :ok
              error -> error
            end

          error ->
            error
        end

      _ ->
        {:error, "Scope must contain exactly one colon"}
    end
  end

  defp validate_action(action) do
    cond do
      action == "" -> {:error, "Action cannot be empty"}
      !String.match?(action, ~r/^[a-zA-Z_][a-zA-Z0-9_]*$/) -> {:error, "Invalid action format"}
      true -> :ok
    end
  end

  defp validate_resource(resource) do
    cond do
      resource == "" -> {:error, "Resource cannot be empty"}
      !String.match?(resource, ~r/^[a-zA-Z_][a-zA-Z0-9_.]*$/) -> {:error, "Invalid resource format"}
      true -> :ok
    end
  end
end

defmodule FraiseQL.ScopeValidationError do
  defexception [:message]

  def exception(reason) when is_binary(reason) do
    %__MODULE__{message: reason}
  end

  def exception(:empty_scope) do
    %__MODULE__{message: "Scope cannot be empty"}
  end

  def exception({:invalid_action, action}) do
    %__MODULE__{message: "Invalid action in scope: #{action}"}
  end

  def exception({:invalid_resource, resource}) do
    %__MODULE__{message: "Invalid resource in scope: #{resource}"}
  end

  def exception({:missing_colon, scope}) do
    %__MODULE__{message: "Scope must contain colon: #{scope}"}
  end

  def exception({:conflict, field}) do
    %__MODULE__{message: "Field #{field} cannot have both requires_scope and requires_scopes"}
  end

  def exception({:invalid_format, scope}) do
    %__MODULE__{message: "Invalid scope format: #{scope}"}
  end
end
