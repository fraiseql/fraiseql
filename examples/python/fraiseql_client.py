#!/usr/bin/env python3
"""FraiseQL Arrow Flight client for Python.

Pulls GraphQL results out of a FraiseQL server as Arrow record batches over
Flight (gRPC), with no JSON on the wire and no row-by-row deserialization. What
comes back is a `pyarrow.Table`, which hands straight to pandas, Polars, DuckDB
or a Parquet file.

    python3 fraiseql_client.py query '{ users { id name email } }'
    python3 fraiseql_client.py query '{ posts { title } }' --output posts.parquet
    python3 fraiseql_client.py view va_orders --limit 100000

Authentication is not optional. The server's `do_get` authenticates before it
even decodes the ticket, so every call needs a session token:

    export FRAISEQL_FLIGHT_TOKEN='<jwt from your identity provider>'

The client exchanges that for a short-lived session token via the Flight
`handshake` RPC, then sends it as `authorization: Bearer …` on every call. If
your deployment mints session tokens some other way, set
`FRAISEQL_FLIGHT_SESSION_TOKEN` instead and the handshake is skipped.

Two ticket types this client does NOT expose, because the server does not serve
them from a stock deployment:

* `ObserverEvents` — `Status::unimplemented`. Query historical events through the
  GraphQL API instead.
* `BulkExport` — fail-closed behind an operator allow-list
  (`with_bulk_export_tables`); a stock server answers `permission_denied`.

See ../r for the same client in R and ../rust/flight_client for Rust.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

try:
    import pyarrow as pa
    from pyarrow import flight
except ImportError:  # pragma: no cover - the message is the point
    sys.exit(
        "pyarrow is required: pip install -r requirements.txt\n"
        "(Arrow Flight needs the full pyarrow wheel, not pyarrow-core.)"
    )

DEFAULT_HOST = "localhost"
DEFAULT_PORT = 50051


class FraiseQLFlightClient:
    """A Flight client that has completed the handshake and holds a session token."""

    def __init__(self, client: flight.FlightClient, session_token: str) -> None:
        self._client = client
        # Every do_get carries this. Without it the server answers UNAUTHENTICATED
        # before it looks at the ticket.
        self._options = flight.FlightCallOptions(
            headers=[(b"authorization", f"Bearer {session_token}".encode())]
        )

    @classmethod
    def connect(
        cls,
        host: str = DEFAULT_HOST,
        port: int = DEFAULT_PORT,
        token: str | None = None,
        session_token: str | None = None,
    ) -> FraiseQLFlightClient:
        """Open a connection and obtain a session token.

        `session_token` short-circuits the handshake; otherwise `token` (a JWT
        your identity provider issued) is exchanged for one.
        """
        client = flight.connect(f"grpc://{host}:{port}")

        if session_token is None:
            if not token:
                raise ValueError(
                    "No credentials. Set FRAISEQL_FLIGHT_TOKEN to a JWT, or "
                    "FRAISEQL_FLIGHT_SESSION_TOKEN to a session token minted elsewhere."
                )
            session_token = cls._handshake(client, token)

        return cls(client, session_token)

    @staticmethod
    def _handshake(client: flight.FlightClient, token: str) -> str:
        """Exchange a JWT for a session token.

        The server expects the literal payload `Bearer <jwt>` and answers with the
        session token as raw bytes.
        """
        for response in client.do_handshake(
            flight.FlightCallOptions(),
            f"Bearer {token}".encode(),
        ):
            return response.decode()
        raise RuntimeError("handshake returned no response")

    def query(self, graphql: str, variables: dict | None = None) -> pa.Table:
        """Execute a GraphQL query and return the result as an Arrow table."""
        return self._fetch({"type": "GraphQLQuery", "query": graphql, "variables": variables})

    def view(
        self,
        view: str,
        *,
        order_by: str | None = None,
        limit: int | None = None,
        offset: int | None = None,
    ) -> pa.Table:
        """Read a pre-compiled Arrow view (`va_*`) directly.

        Faster than `query` because the Arrow schema is known at compile time and
        no type inference happens per request. `filter` is deliberately not
        exposed: it is a raw WHERE clause, and this client will not build one for
        you out of untrusted input.
        """
        return self._fetch(
            {
                "type": "OptimizedView",
                "view": view,
                "filter": None,
                "order_by": order_by,
                "limit": limit,
                "offset": offset,
            }
        )

    def _fetch(self, ticket: dict) -> pa.Table:
        """Send one ticket and read the whole stream into a table."""
        reader = self._client.do_get(flight.Ticket(json.dumps(ticket).encode()), self._options)
        return reader.read_all()


def _credentials() -> tuple[str | None, str | None]:
    return (
        os.environ.get("FRAISEQL_FLIGHT_TOKEN"),
        os.environ.get("FRAISEQL_FLIGHT_SESSION_TOKEN"),
    )


def _write(table: pa.Table, path: str) -> None:
    """Write the table out, choosing the format from the file extension."""
    # These three imports are deliberately lazy: parquet and csv are optional
    # pyarrow components, and paying for them on every `query` that only prints
    # is not worth it.
    if path.endswith(".parquet"):
        from pyarrow import parquet as pq  # noqa: PLC0415

        pq.write_table(table, path)
    elif path.endswith(".csv"):
        from pyarrow import csv  # noqa: PLC0415

        csv.write_csv(table, path)
    elif path.endswith(".arrow"):
        with pa.OSFile(path, "wb") as sink, pa.ipc.new_file(sink, table.schema) as writer:
            writer.write_table(table)
    else:
        raise SystemExit(f"unsupported output extension: {path} (.parquet, .csv, .arrow)")
    print(f"wrote {table.num_rows} rows to {path}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    # The shared options live on a `parents=` parser rather than on the top-level
    # one, so `query ... --output x.parquet` works. Declared only at the top level,
    # argparse accepts them BEFORE the subcommand and nowhere else — which is not
    # where anyone types them.
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--host", default=os.environ.get("FRAISEQL_FLIGHT_HOST", DEFAULT_HOST))
    common.add_argument(
        "--port", type=int, default=int(os.environ.get("FRAISEQL_FLIGHT_PORT", DEFAULT_PORT))
    )
    common.add_argument("--output", help="write to this file (.parquet, .csv or .arrow)")

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0], parents=[common])
    sub = parser.add_subparsers(dest="command", required=True)

    query = sub.add_parser("query", parents=[common], help="execute a GraphQL query")
    query.add_argument("graphql")
    query.add_argument("--variables", help="variables as a JSON object")

    view = sub.add_parser("view", parents=[common], help="read a pre-compiled Arrow view")
    view.add_argument("view")
    view.add_argument("--order-by")
    view.add_argument("--limit", type=int)
    view.add_argument("--offset", type=int)

    args = parser.parse_args(argv)
    token, session_token = _credentials()

    try:
        client = FraiseQLFlightClient.connect(
            args.host, args.port, token=token, session_token=session_token
        )
        if args.command == "query":
            variables = json.loads(args.variables) if args.variables else None
            table = client.query(args.graphql, variables)
        else:
            table = client.view(
                args.view, order_by=args.order_by, limit=args.limit, offset=args.offset
            )
    except ValueError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    except pa.ArrowException as err:
        # `flight.FlightError` is NOT enough: pyarrow maps some gRPC statuses to
        # plain Arrow exceptions instead — NOT_FOUND arrives as `ArrowKeyError`, so
        # an unknown view escapes a `except flight.FlightError` as a traceback.
        # UNAUTHENTICATED means the session token was missing or expired;
        # UNIMPLEMENTED means this server does not serve that ticket type.
        print(f"flight error: {_first_line(err)}", file=sys.stderr)
        return 1

    if args.output:
        _write(table, args.output)
    else:
        print(table.to_pandas().to_string() if _has_pandas() else table)
    return 0


def _first_line(err: BaseException) -> str:
    """The message, without gRPC's multi-line client debug context."""
    return str(err).split(". gRPC client debug context")[0]


def _has_pandas() -> bool:
    try:
        import pandas  # noqa: F401, PLC0415
    except ImportError:
        return False
    return True


if __name__ == "__main__":
    raise SystemExit(main())
