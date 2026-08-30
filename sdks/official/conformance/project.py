"""Project a compiled schema onto the canonical conformance observations.

The conformance suite does not diff whole compiled schemas. A compiled schema carries
SQL templates, filter-operator metadata, content hashes and synthesized helper types —
all of it correct-by-construction and none of it an SDK's business. Diffing it would
make every compiler change break eleven SDK gates at once.

Instead each compiled schema is reduced to a set of **observations**, grouped by the
authorable *construct* that produces them. An observation is a fact an author declared
that survived the whole authoring → export → compile pipeline. That is the only property
the SDKs are being held to, and it is exactly the property the nine defects in this
suite's motivating issue set violated:

* `#849` — C# dropped `relay`/`is_error`, so the `type_relay` observation
  (`interfaces` gains `Node`, `types` gains `UserEdge`/`UserConnection`) never appears.
* `#852` — PHP wrote `invalidates`, so `mutation_invalidates_views` is empty.
* `#855` — Rust emitted a name-keyed map, so `types` is empty while the compile succeeds.

Because the observations are keyed by construct, an SDK that genuinely cannot author a
construct can declare it unsupported in `manifest.json` and have those keys dropped —
loudly, with a published reason — rather than silently failing.
"""

from __future__ import annotations

from typing import Any

# Names the canonical fixture declares. Anything else in a compiled schema is
# synthesized by the compiler (PageInfo, *WhereInput, Node, …) and is not an
# author-visible fact, except where a construct explicitly asserts its synthesis.
AUTHORED_TYPES = ("User", "Order", "UserNotFound")
AUTHORED_INPUT_TYPES = ("CreateUserInput",)
AUTHORED_ENUMS = ("OrderStatus",)
AUTHORED_QUERIES = ("users", "user", "tenantOrders")
AUTHORED_MUTATIONS = ("createUser", "placeOrder")
AUTHORED_SUBSCRIPTIONS = ("orderUpdated",)

# The pgvector type is kept out of `AUTHORED_TYPES` on purpose: the `vector_fields`
# construct owns it whole. A `Vector` field without a `vector_config` is a compile
# error, so an SDK that cannot author the config cannot author the type either —
# were it listed above, declaring the gap would still leave it failing `types`,
# which reads as "this SDK's types are broken" rather than "this SDK has no vector
# surface". Owning the type makes the gap declarable in one place, the way
# `type_relay` owns the connection types the compiler synthesizes.
AUTHORED_VECTOR_TYPES = ("Document",)

# The `type_crud` construct owns `Ticket` whole, the way `vector_fields` owns `Document`:
# the type, the two input objects and the five operations an SDK's CRUD generator must
# produce from one `crud` declaration. Listing them in AUTHORED_TYPES / _QUERIES /
# _MUTATIONS instead would spread one construct across five, so an SDK with no CRUD
# surface would fail all five and read as "this SDK's queries are broken".
#
# `crud` is an authoring-time expansion — `IntermediateType` has no such member — so the
# ONLY evidence an SDK implements it is that these appear in the compiled schema. That is
# why nine SDKs drifted three separate ways behind green suites: Dart's generator had no
# caller at all and Ruby's only its own tests (#1241, #1242), Python emitted
# `create_ticket` where the other eight emitted `fn_create_ticket` (#1243), and three of
# the nine emitted flat arguments where six emitted an input object (#1246).
# Two words, deliberately. A one-word type name spells the same in snake_case and
# camelCase, so a `Ticket` fixture would have passed for the six SDKs whose generated
# operations are snake_case while every hand-authored operation beside them is camelCase
# (#1247) — a suite uniform in the dimension that selects the branch tests one branch.
# `due_date` does the same for the generated input objects' field names.
CRUD_TYPE = "SupportTicket"
CRUD_INPUT_TYPES = ("CreateSupportTicketInput", "UpdateSupportTicketInput")
CRUD_QUERIES = ("supportTicket", "supportTickets")
CRUD_MUTATIONS = ("createSupportTicket", "updateSupportTicket", "deleteSupportTicket")

# Every construct the canonical fixture exercises. An SDK must satisfy each one or
# declare it unsupported in `manifest.json` with a reason.
#
# Adding an entry here is how the suite grows: a new construct fails every SDK that
# has not implemented it until each either implements it or declares the gap. That is
# the intended behaviour, and it follows from two places — `project()` refuses to return
# unless every listed construct produced an observation, and `diff_observations` skips a
# construct only where the SDK's manifest declares it unsupported.
#
# Both are pinned by `selftest.py`, which also covers the stale-declaration check and
# the unknown-key check (`make test-conformance-selftest`, in preflight and ShellGates).
# ⚠ That file is named here again only because it now exists: this comment previously
# cited a `selftest.py` that had never been written, so it read as evidence and was
# none (#1118). Check before citing.
CONSTRUCTS = (
    "types",
    "field_description",
    "field_scope",
    "field_deprecated",
    "type_relay",
    "type_is_error",
    "type_crud",
    "input_types",
    "enums",
    "queries",
    "query_arguments",
    "query_inject_params",
    "query_cache_ttl",
    "query_requires_role",
    "mutations",
    "mutation_arguments",
    "mutation_invalidates_views",
    "mutation_invalidates_fact_tables",
    "subscriptions",
    "vector_fields",
)


def _by_name(items: Any) -> dict[str, dict[str, Any]]:
    """Index a compiled schema section by `name`, tolerating absence."""
    if not isinstance(items, list):
        return {}
    return {item["name"]: item for item in items if isinstance(item, dict) and "name" in item}


def _fields(type_def: dict[str, Any]) -> list[dict[str, Any]]:
    """A type's fields reduced to (name, type, nullable) triples, in declared order."""
    return [
        {
            "name": f.get("name"),
            "type": f.get("field_type"),
            "nullable": f.get("nullable"),
        }
        for f in type_def.get("fields", [])
        if isinstance(f, dict)
    ]


def _vector_fields(type_def: dict[str, Any]) -> list[dict[str, Any]]:
    """A vector type's fields, carrying what makes them vector fields.

    `vector_distance` travels with them: it names the vector field whose search
    distance a `Float` field carries, the compiler resolves that name against the
    type's own fields, and a dangling reference is a compile error — so the pair
    only ever appears together.
    """
    return [
        {
            "name": f.get("name"),
            "type": f.get("field_type"),
            "nullable": f.get("nullable"),
            "vector_config": f.get("vector_config"),
            "vector_distance": f.get("vector_distance"),
        }
        for f in type_def.get("fields", [])
        if isinstance(f, dict)
    ]


def _operation_kind(mutation: dict[str, Any]) -> str | None:
    """The mutation's DML verb.

    The compiled form is an externally-tagged enum — `{"Insert": {"table": "..."}}` —
    so the verb is the sole key. Reduced to the bare verb because the table travels
    separately as `sql_source` and asserting it twice would double-count one fact.
    """
    operation = mutation.get("operation")
    if isinstance(operation, dict) and len(operation) == 1:
        return next(iter(operation))
    if isinstance(operation, str):
        return operation
    return None


def project(compiled: dict[str, Any]) -> dict[str, Any]:
    """Reduce a compiled schema to the canonical observations, keyed by construct.

    Missing sections project to empty rather than raising: an SDK that emits no
    mutations at all must produce a *visible, diffable* absence, not a crash that
    reads the same as a harness bug.
    """
    types = _by_name(compiled.get("types"))
    input_types = _by_name(compiled.get("input_types"))
    enums = _by_name(compiled.get("enums"))
    interfaces = _by_name(compiled.get("interfaces"))
    queries = _by_name(compiled.get("queries"))
    mutations = _by_name(compiled.get("mutations"))
    subscriptions = _by_name(compiled.get("subscriptions"))

    observations: dict[str, Any] = {}

    observations["types"] = {
        name: {
            "sql_source": types[name].get("sql_source"),
            "fields": _fields(types[name]),
        }
        for name in AUTHORED_TYPES
        if name in types
    }

    # Field-level metadata is split out from `types` so an SDK that carries fields but
    # drops descriptions or scopes fails precisely that construct. Folding them into
    # `types` would make one missing scope read as "types are broken".
    observations["field_description"] = {
        f"{tname}.{f['name']}": f["description"]
        for tname in AUTHORED_TYPES
        if tname in types
        for f in types[tname].get("fields", [])
        if isinstance(f, dict) and f.get("description")
    }

    observations["field_scope"] = {
        f"{tname}.{f['name']}": f["requires_scope"]
        for tname in AUTHORED_TYPES
        if tname in types
        for f in types[tname].get("fields", [])
        if isinstance(f, dict) and f.get("requires_scope")
    }

    # The compiled key is `deprecation`, not the authored `deprecated`, and it is what
    # `isDeprecated` / `deprecationReason` resolve from — so this asserts the fact
    # reached introspection, which is the whole point of #1025 having implemented it
    # rather than dropped it. The fixture marks a field that also carries a description,
    # so neither field-level construct can pass by carrying the other.
    observations["field_deprecated"] = {
        f"{tname}.{f['name']}": f["deprecation"]
        for tname in AUTHORED_TYPES
        if tname in types
        for f in types[tname].get("fields", [])
        if isinstance(f, dict) and f.get("deprecation")
    }

    # `relay: true` is not asserted on the authored type alone — the compiler *acts* on
    # it, and the action is the point. A schema that carried the flag but synthesized no
    # Node interface and no Connection would be a compiler defect this suite should show.
    observations["type_relay"] = {
        "flagged": sorted(name for name in AUTHORED_TYPES if types.get(name, {}).get("relay")),
        "implements_node": sorted(
            name for name in AUTHORED_TYPES if "Node" in types.get(name, {}).get("implements", [])
        ),
        "node_interface_synthesized": "Node" in interfaces,
        "connection_types_synthesized": sorted(
            name for name in ("UserEdge", "UserConnection", "PageInfo") if name in types
        ),
    }

    observations["type_is_error"] = sorted(
        name for name in AUTHORED_TYPES if types.get(name, {}).get("is_error")
    )

    # Asserted as one observation because one declaration produces all of it. The
    # `computed` half is only visible here: the flag itself is never emitted (the compiler
    # denies unknown fields), so the sole evidence an SDK honoured it is that `slug` is on
    # the type and absent from both input objects. A projection that read the type alone
    # would pass for an SDK that put a server-assigned field in `CreateTicketInput`.
    observations["type_crud"] = {
        # `{}` rather than a dict of Nones when the type is absent: `run.py::exercised`
        # asks whether ANY sub-observation is truthy, and a dict whose values are all
        # empty is still a truthy dict. Filled in, the `minimal` fixture — which declares
        # no CRUD at all — would count as exercising this construct, and every SDK that
        # declared the gap would then be reported as declaring it falsely.
        "type": (
            {"sql_source": types[CRUD_TYPE].get("sql_source"), "fields": _fields(types[CRUD_TYPE])}
            if CRUD_TYPE in types
            else {}
        ),
        "input_types": {
            name: {"fields": _fields(input_types[name])}
            for name in CRUD_INPUT_TYPES
            if name in input_types
        },
        "queries": {
            name: {
                "return_type": queries[name].get("return_type"),
                "returns_list": queries[name].get("returns_list"),
                "nullable": queries[name].get("nullable"),
                "sql_source": queries[name].get("sql_source"),
                "auto_params": queries[name].get("auto_params"),
                "arguments": [
                    {"name": a.get("name"), "type": a.get("arg_type"), "nullable": a.get("nullable")}
                    for a in queries[name].get("arguments", [])
                    if isinstance(a, dict)
                ],
            }
            for name in CRUD_QUERIES
            if name in queries
        },
        "mutations": {
            name: {
                "return_type": mutations[name].get("return_type"),
                "operation": _operation_kind(mutations[name]),
                "sql_source": mutations[name].get("sql_source"),
                "arguments": [
                    {"name": a.get("name"), "type": a.get("arg_type"), "nullable": a.get("nullable")}
                    for a in mutations[name].get("arguments", [])
                    if isinstance(a, dict)
                ],
            }
            for name in CRUD_MUTATIONS
            if name in mutations
        },
    }

    observations["input_types"] = {
        name: {"fields": _fields(input_types[name])}
        for name in AUTHORED_INPUT_TYPES
        if name in input_types
    }

    observations["enums"] = {
        name: [v.get("name") for v in enums[name].get("values", []) if isinstance(v, dict)]
        for name in AUTHORED_ENUMS
        if name in enums
    }

    observations["queries"] = {
        name: {
            "return_type": queries[name].get("return_type"),
            "returns_list": queries[name].get("returns_list"),
            "nullable": queries[name].get("nullable"),
            "sql_source": queries[name].get("sql_source"),
        }
        for name in AUTHORED_QUERIES
        if name in queries
    }

    observations["query_arguments"] = {
        name: [
            {"name": a.get("name"), "type": a.get("arg_type"), "nullable": a.get("nullable")}
            for a in queries[name].get("arguments", [])
            if isinstance(a, dict)
        ]
        for name in AUTHORED_QUERIES
        if name in queries
    }

    observations["query_inject_params"] = {
        name: queries[name]["inject_params"]
        for name in AUTHORED_QUERIES
        if queries.get(name, {}).get("inject_params")
    }

    observations["query_cache_ttl"] = {
        name: queries[name]["cache_ttl_seconds"]
        for name in AUTHORED_QUERIES
        if queries.get(name, {}).get("cache_ttl_seconds") is not None
    }

    observations["query_requires_role"] = {
        name: queries[name]["requires_role"]
        for name in AUTHORED_QUERIES
        if queries.get(name, {}).get("requires_role")
    }

    observations["mutations"] = {
        name: {
            "return_type": mutations[name].get("return_type"),
            "operation": _operation_kind(mutations[name]),
            "sql_source": mutations[name].get("sql_source"),
        }
        for name in AUTHORED_MUTATIONS
        if name in mutations
    }

    observations["mutation_arguments"] = {
        name: [
            {"name": a.get("name"), "type": a.get("arg_type"), "nullable": a.get("nullable")}
            for a in mutations[name].get("arguments", [])
            if isinstance(a, dict)
        ]
        for name in AUTHORED_MUTATIONS
        if name in mutations
    }

    observations["mutation_invalidates_views"] = {
        name: mutations[name]["invalidates_views"]
        for name in AUTHORED_MUTATIONS
        if mutations.get(name, {}).get("invalidates_views")
    }

    observations["mutation_invalidates_fact_tables"] = {
        name: mutations[name]["invalidates_fact_tables"]
        for name in AUTHORED_MUTATIONS
        if mutations.get(name, {}).get("invalidates_fact_tables")
    }

    # A subscription is asserted whole rather than by presence. Every SDK that shipped one
    # emitted `entity_type`/`nullable`/`operation` against an `IntermediateSubscription`
    # that has none of them and denies unknown fields, so three SDKs failed the compile and
    # two — PHP and Java — emitted no `subscriptions` key at all and had the section
    # silently dropped from a compile that then reported success (#1024). Nothing caught
    # either, because this construct did not exist and the conformance suite is the only
    # gate that compiles SDK output.
    #
    # `filter` is projected as the compiler lowered it: the authored
    # `{conditions: [{argument, path}]}` becomes `{argument_paths: {...}}`, so an SDK that
    # emitted the authoring shape verbatim into a place the compiler does not read it
    # would show an empty map here rather than a match.
    observations["subscriptions"] = {
        name: {
            "return_type": subscriptions[name].get("return_type"),
            "description": subscriptions[name].get("description"),
            "topic": subscriptions[name].get("topic"),
            "arguments": [
                {"name": a.get("name"), "type": a.get("arg_type"), "nullable": a.get("nullable")}
                for a in subscriptions[name].get("arguments", [])
                if isinstance(a, dict)
            ],
            "argument_paths": (subscriptions[name].get("filter") or {}).get("argument_paths"),
            "fields": subscriptions[name].get("fields"),
        }
        for name in AUTHORED_SUBSCRIPTIONS
        if name in subscriptions
    }

    # Every key of `vector_config` is asserted, not just its presence, because two of
    # the three have serde defaults: an SDK that emits `dimensions` alone compiles to
    # hnsw + cosine and would pass a presence check while having silently chosen the
    # index and the metric for the author. The fixture therefore declares a non-default
    # `index_type` and `distance_metric` on every field.
    #
    # The field type is asserted here too, and not left to `types`, because it is what
    # the compiler acted on: `vector_config` is refused on a non-vector field, a binary
    # metric is refused on a float vector and vice versa, and `ivf_flat` is refused where
    # pgvector ships no operator class — so a config that survives next to its declared
    # type is a config the compiler checked against pgvector's own table.
    observations["vector_fields"] = {
        name: {
            "sql_source": types[name].get("sql_source"),
            "fields": _vector_fields(types[name]),
        }
        for name in AUTHORED_VECTOR_TYPES
        if name in types
    }

    missing = set(CONSTRUCTS) - set(observations)
    if missing:
        raise AssertionError(
            f"project() declares constructs it does not emit: {sorted(missing)}. "
            "Every entry in CONSTRUCTS must produce an observation key."
        )
    return observations
