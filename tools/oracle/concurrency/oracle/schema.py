"""The fixture's authoring schema, with or without a cache TTL on every query."""

from __future__ import annotations

from typing import Any

TENANT = {"tenant_id": "jwt:tenant_id"}


def _field(name: str, type_: str, *, nullable: bool = False, **extra: Any) -> dict[str, Any]:
    return {"name": name, "type": type_, "nullable": nullable, **extra}


def _version_view(name: str, source: str) -> dict[str, Any]:
    return {
        "name": name,
        "sql_source": source,
        "fields": [
            _field("id", "ID"),
            _field("fkAccount", "ID"),
            _field("accountVersion", "Int", nullable=True),
        ],
    }


TYPES: list[dict[str, Any]] = [
    {
        "name": "Account",
        "sql_source": "v_account",
        "fields": [
            _field("id", "ID"),
            _field("tenantId", "String"),
            _field("name", "String"),
            _field("version", "Int"),
            _field("secret", "String", requires_scope="read:secret"),
            _field("seenTenant", "String", nullable=True),
        ],
    },
    {
        "name": "AccountRef",
        "fields": [
            _field("id", "ID"),
            _field("version", "Int"),
            _field("seenTenant", "String", nullable=True),
        ],
    },
    {
        "name": "AccountSummary",
        "sql_source": "v_account_summary",
        "fields": [_field("id", "ID"), _field("version", "Int")],
    },
    {
        "name": "Item",
        "sql_source": "v_item",
        "fields": [
            _field("id", "ID"),
            _field("tenantId", "String"),
            _field("fkAccount", "ID"),
            _field("seq", "Int"),
            _field("label", "String"),
            _field("accountVersion", "Int", nullable=True),
            _field("account", "AccountRef", nullable=True),
        ],
    },
    _version_view("NoteFn", "v_note_fn"),
    _version_view("NoteAtomic", "v_note_atomic"),
    {
        "name": "ItemFn",
        "sql_source": "v_item_fn",
        "fields": [
            _field("id", "ID"),
            _field("fkAccount", "ID"),
            _field("seq", "Int"),
            _field("accountVersion", "Int", nullable=True),
        ],
    },
]

ID_ARG = [_field("id", "ID")]


def build(ttl: int | None) -> dict[str, Any]:
    """The authoring schema. `ttl` None leaves every query without `cache_ttl_seconds`."""

    def query(
        name: str,
        return_type: str,
        source: str,
        *,
        many: bool,
        args: list[dict[str, Any]] | None = None,
        inject: bool = True,
        **extra: Any,
    ) -> dict[str, Any]:
        q: dict[str, Any] = {
            "name": name,
            "return_type": return_type,
            "returns_list": many,
            "sql_source": source,
        }
        if not many:
            q["nullable"] = True
        if args:
            q["arguments"] = args
        if inject:
            q["inject_params"] = dict(TENANT)
        if ttl is not None:
            q["cache_ttl_seconds"] = ttl
        return {**q, **extra}

    def mutation(
        name: str, return_type: str, function: str, operation: str, args: list[dict[str, Any]]
    ) -> dict[str, Any]:
        return {
            "name": name,
            "return_type": return_type,
            "sql_source": function,
            "operation": operation,
            "arguments": args,
            "inject_params": dict(TENANT),
        }

    return {
        "types": TYPES,
        "enums": [],
        "queries": [
            query("account", "Account", "v_account", many=False, args=ID_ARG),
            query("accounts", "Account", "v_account", many=True),
            query("accountSummaries", "AccountSummary", "v_account_summary", many=True),
            query("items", "Item", "v_item", many=True),
            query("notesFn", "NoteFn", "v_note_fn", many=True),
            query("notesAtomic", "NoteAtomic", "v_note_atomic", many=True),
            query("itemsFn", "ItemFn", "v_item_fn", many=True),
            query("itemsConn", "Item", "v_item", many=True, relay=True),
            query("rlsAccounts", "Account", "v_account_rls", many=True, inject=False),
        ],
        "mutations": [
            mutation("bumpAccount", "Account", "fn_bump_account", "UPDATE", ID_ARG),
            mutation("bumpFail", "Account", "fn_bump_fail", "UPDATE", ID_ARG),
            mutation(
                "slowBump", "Account", "fn_slow_bump", "UPDATE", [*ID_ARG, _field("ms", "Int")]
            ),
            mutation(
                "recordEffect",
                "Account",
                "fn_record_effect",
                "CREATE",
                [_field("tag", "String"), _field("ms", "Int")],
            ),
            mutation(
                "createItem",
                "Item",
                "fn_create_item",
                "CREATE",
                [_field("fkAccount", "ID"), _field("seq", "Int")],
            ),
            mutation("deleteItem", "Item", "fn_delete_item", "DELETE", ID_ARG),
        ],
        "subscriptions": [
            {"name": "accountChanged", "return_type": "Account", "topic": "Account"},
            {"name": "itemCreated", "return_type": "Item", "topic": "Item"},
        ],
        "naming_convention": "camelCase",
    }
