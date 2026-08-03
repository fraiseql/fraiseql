"""Shared operation-spec extraction for the AI-framework adapters.

Every adapter (LangChain, LlamaIndex, OpenAI function calling, raw MCP) consumes
the same normalised intermediate representation produced here from a standard
GraphQL introspection result, so tool names, argument types, and generated
documents cannot drift between frameworks.

This module has no dependencies beyond the standard library.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

_SCALAR_JSON_TYPES: dict[str, str] = {
    "Int": "integer",
    "Float": "number",
    "Boolean": "boolean",
    "String": "string",
    "ID": "string",
    "UUID": "string",
    "DateTime": "string",
    "Date": "string",
    "Time": "string",
    "Decimal": "string",
}


@dataclass(frozen=True)
class ArgSpec:
    """One operation argument, normalised from introspection."""

    name: str
    graphql_type: str
    """Rendered GraphQL type reference (e.g. ``ID!``, ``[String]``)."""
    json_schema: dict[str, Any]
    required: bool
    description: str | None = None


@dataclass(frozen=True)
class OperationSpec:
    """One root query or mutation, normalised from introspection."""

    name: str
    kind: str
    """``"query"`` or ``"mutation"``."""
    description: str | None
    args: list[ArgSpec] = field(default_factory=list)

    @property
    def document(self) -> str:
        """The GraphQL document invoking this operation with typed variables."""
        if not self.args:
            return f"{self.kind} {{ {self.name} }}"
        var_decls = ", ".join(f"${a.name}: {a.graphql_type}" for a in self.args)
        call_args = ", ".join(f"{a.name}: ${a.name}" for a in self.args)
        return f"{self.kind} ({var_decls}) {{ {self.name}({call_args}) }}"

    @property
    def parameters_schema(self) -> dict[str, Any]:
        """The arguments as a JSON-Schema object (OpenAI / MCP input shape)."""
        properties: dict[str, Any] = {}
        required: list[str] = []
        for arg in self.args:
            schema = dict(arg.json_schema)
            if arg.description:
                schema["description"] = arg.description
            properties[arg.name] = schema
            if arg.required:
                required.append(arg.name)
        schema_obj: dict[str, Any] = {"type": "object", "properties": properties}
        if required:
            schema_obj["required"] = required
        return schema_obj

    @property
    def display_description(self) -> str:
        """The description, with an argument summary appended when args exist."""
        base = self.description or f"Execute the FraiseQL {self.kind} `{self.name}`"
        if not self.args:
            return base
        arg_desc = ", ".join(f"{a.name}: {a.graphql_type}" for a in self.args)
        return f"{base}. Arguments (JSON): {arg_desc}"


def _render_type(type_ref: dict[str, Any] | None) -> str:
    """Render an introspection type ref to a GraphQL type reference string."""
    if not type_ref:
        return "String"
    kind = type_ref.get("kind")
    if kind == "NON_NULL":
        return f"{_render_type(type_ref.get('ofType'))}!"
    if kind == "LIST":
        return f"[{_render_type(type_ref.get('ofType'))}]"
    return type_ref.get("name") or "String"


def _json_schema_for(type_ref: dict[str, Any] | None) -> dict[str, Any]:
    """Map an introspection type ref onto a JSON-Schema fragment."""
    if not type_ref:
        return {"type": "string"}
    kind = type_ref.get("kind")
    if kind == "NON_NULL":
        return _json_schema_for(type_ref.get("ofType"))
    if kind == "LIST":
        return {"type": "array", "items": _json_schema_for(type_ref.get("ofType"))}
    if kind in ("INPUT_OBJECT", "OBJECT"):
        return {"type": "object"}
    if kind == "ENUM":
        return {"type": "string"}
    name = type_ref.get("name") or ""
    return {"type": _SCALAR_JSON_TYPES.get(name, "string")}


def _is_required(type_ref: dict[str, Any] | None, default_value: Any) -> bool:
    """Non-null without a default ⇒ the caller must supply the argument."""
    return bool(type_ref) and type_ref.get("kind") == "NON_NULL" and default_value is None


def operation_specs(
    schema_data: dict[str, Any],
    *,
    include: list[str] | None = None,
    exclude: list[str] | None = None,
) -> list[OperationSpec]:
    """Extract root query/mutation specs from an introspection result.

    Args:
        schema_data: The introspection payload (``client.introspect()`` result).
        include: Whitelist of operation names (None = all).
        exclude: Blacklist of operation names (None = none).
    """
    specs: list[OperationSpec] = []
    schema_info = schema_data.get("data", {}).get("__schema", {})

    for type_info in schema_info.get("types", []):
        type_name = type_info.get("name", "")
        if type_name == "Query":
            kind = "query"
        elif type_name == "Mutation":
            kind = "mutation"
        else:
            continue

        for op_field in type_info.get("fields", []) or []:
            name = op_field["name"]
            if name.startswith("__"):
                continue
            if include and name not in include:
                continue
            if exclude and name in exclude:
                continue

            args = [
                ArgSpec(
                    name=a["name"],
                    graphql_type=_render_type(a.get("type")),
                    json_schema=_json_schema_for(a.get("type")),
                    required=_is_required(a.get("type"), a.get("defaultValue")),
                    description=a.get("description"),
                )
                for a in op_field.get("args", []) or []
            ]
            specs.append(
                OperationSpec(
                    name=name,
                    kind=kind,
                    description=op_field.get("description"),
                    args=args,
                )
            )

    return specs
