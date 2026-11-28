# graphql-js differential oracle

FraiseQL and [graphql-js](https://github.com/graphql/graphql-js), the GraphQL reference
implementation, answer the same documents over the same schema and the same rows. Where
they disagree, FraiseQL is usually wrong. The oracle runs the comparison, so a spec
regression is caught by executing documents, not by reading code.

## What it runs

- **Targeted cases** (`lib/cases.js`): one document per spec behaviour (introspection,
  aliases and field merging, fragments, `@skip`/`@include`, variable and input coercion,
  enums, custom scalars, operation selection, null propagation, syntax).
- **A seeded random corpus** (`lib/fuzz.js`): 4000 documents for each of the seeds 11
  and 12. About 15% are mutated to break one validation rule each (`mut:<rule>` tags).
  The generator is a pure function of its seed, so every machine gets the same documents.
- **An introspection diff**: the full introspection query against both servers, one line
  per differing schema key, type or directive.
- **The graphql-http audit** (GraphQL-over-HTTP spec) against FraiseQL. Only `warn` and
  `error` results count.

The reference server (`lib/reference.js`) serves `fixture/schema.graphql`, the SDL that
FraiseQL is expected to expose for `fixture/schema.json`. It resolves against the same
database rows, implementing the `where`, `orderBy`, `limit` and `offset` arguments
directly. Messages are never compared. Error locations, error counts and non-standard
error keys are not ranked yet.

## The baseline

`baseline.json` records every current disagreement:

- each document's fingerprint and its issue set;
- each introspection line;
- each audit finding.

The gate is equality in both directions:

- **A disagreement not in the baseline** is a regression, and the run fails. It prints
  the document and both responses.
- **A baseline entry that no longer occurs** is progress, and the run fails until the
  baseline records it. Rerun with `--bless` and commit `baseline.json` in the same change.

The baseline can therefore only shrink toward full agreement, and every change to it is
a reviewed diff.

If FraiseQL's generated API changes on purpose (a new filter operator, say), regenerate
the expected SDL with `node oracle.js --server <url> --bless-sdl`, review the
`fixture/schema.graphql` diff, then re-bless.

## Running it

```bash
make oracle-graphql-js            # release build, then the oracle (needs Docker)
make oracle-graphql-js-selftest   # proves the gate goes red (below)
tools/oracle/graphql-js/run.sh --report /tmp/oracle.json   # every disagreement, both responses
```

`run.sh` reads the following:

| Variable | Meaning |
|---|---|
| `FRAISEQL_CLI`, `FRAISEQL_SERVER` | The binaries under test. Default: `target/release/*`. |
| `ORACLE_ADMIN_URL` | A PostgreSQL URL allowed to create databases. When unset, a throwaway PostgreSQL 18 container is started and removed on exit. |
| `ORACLE_PRIVATE_MODULE` | A module exporting `{ cases, mutations }`: extra targeted cases, and replacements for fuzz mutation slots by name. Kept outside the repository, and run against its own `--baseline`. |

In CI, the Dagger function `graphql-js-oracle` (`.dagger/oracle.go`) runs the same script
against the integration legs' PostgreSQL 18 service.

Exit codes: 0 means the run matches the baseline; 1 means the gate failed; 2 means the
harness itself failed.

## Self-test

`run.sh --selftest` checks that the gate can fail, against a server that matches the
baseline:

- Every gate site fails when the baseline is changed by one entry, and names that entry.
  The gate sites are disagreements, introspection and audit, each in both directions,
  plus a changed corpus.
- A proxy in front of FraiseQL starts executing documents that carry an unknown directive.
  They must be reported, and nothing else.
- The proxy returns one wrong value. Exactly the affected documents must be reported.
