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
    selection: tuple[str, ...] = ()
    """Leaf fields of the return type, empty when the root field returns a leaf.

    A composite root field selected with no sub-selection is not an error the server
    reports: it answers HTTP 200 with no ``errors`` and one **empty object** per row,
    because the projection walks zero fields. Every adapter tool therefore reported
    success and handed the model nothing (#1076).
    """

    @property
    def document(self) -> str:
        """The GraphQL document invoking this operation with typed variables."""
        selection = f" {{ {' '.join(self.selection)} }}" if self.selection else ""
        if not self.args:
            return f"{self.kind} {{ {self.name}{selection} }}"
        var_decls = ", ".join(f"${a.name}: {a.graphql_type}" for a in self.args)
        call_args = ", ".join(f"{a.name}: ${a.name}" for a in self.args)
        return f"{self.kind} ({var_decls}) {{ {self.name}({call_args}){selection} }}"

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


# Kinds that must carry a sub-selection, and kinds that must not. Anything else — a
# truncated type ref, or a name the payload does not define — resolves to neither and
# gets no selection, because inventing one produces a document the server rejects
# outright, which is worse than the one it wrongly accepts.
_COMPOSITE_KINDS = frozenset({"OBJECT", "INTERFACE", "UNION"})
_LEAF_KINDS = frozenset({"SCALAR", "ENUM"})


def _named_type(type_ref: dict[str, Any] | None) -> dict[str, Any] | None:
    """Unwrap NON_NULL/LIST down to the named type, or None if the chain is truncated.

    ``client.introspect`` asks for three levels of wrapping, which is exactly what the
    server publishes for its deepest shape (``[User]!`` → NON_NULL/LIST/OBJECT). A
    deeper chain would arrive with no ``ofType`` and no name, and is reported as
    unresolvable rather than guessed at.
    """
    while type_ref and type_ref.get("kind") in ("NON_NULL", "LIST"):
        type_ref = type_ref.get("ofType")
    return type_ref or None


def _leaf_selection(
    return_type: dict[str, Any] | None, types_by_name: dict[str, dict[str, Any]]
) -> tuple[str, ...]:
    """The return type's leaf fields, or ``()`` when the root field returns a leaf.

    Only scalar and enum fields are selected. A composite field would need a
    sub-selection of its own, which is the same defect one level down — and recursing
    would need a cycle policy for a self-referential type, which nothing here can
    choose for the caller.

    A composite with no leaf fields selects ``__typename``: it is the one field
    guaranteed to exist on every composite and to be a leaf, and ``{ }`` does not parse.
    """
    named = _named_type(return_type)
    if not named or named.get("kind") not in _COMPOSITE_KINDS:
        return ()

    type_def = types_by_name.get(named.get("name") or "")
    if type_def is None:
        # A UNION has no `fields` and an undeclared name resolves to nothing. Only the
        # former is safely selectable, and only via `__typename`.
        return ("__typename",) if named.get("kind") == "UNION" else ()

    leaves = tuple(
        f["name"]
        for f in type_def.get("fields") or []
        if not f.get("name", "").startswith("__")
        and (_named_type(f.get("type")) or {}).get("kind") in _LEAF_KINDS
    )
    return leaves or ("__typename",)


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

    # Indexed first: a root field's return type is resolved against the same payload,
    # which has always carried every type's full field list. `operation_specs` simply
    # threw `op_field["type"]` away, so the data needed to build a selection set was
    # present all along (#1076).
    types_by_name = {
        t["name"]: t
        for t in schema_info.get("types", []) or []
        if isinstance(t, dict) and "name" in t
    }

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
                    selection=_leaf_selection(op_field.get("type"), types_by_name),
                )
            )

    return specs
