# fraiseql-ruby (official)

> **Status: Not yet implemented.**

The official v2 Ruby SDK for FraiseQL is a planned future addition.

A community v1 Ruby SDK exists at [`sdks/community/fraiseql-ruby/`](../../community/fraiseql-ruby/) but is
not compatible with the v2 compiled schema format and is no longer maintained.

## Contributing

If you would like to contribute a v2 Ruby SDK, please open a discussion on the main repository.
The SDK must:

- Parse and validate `schema.compiled.json` produced by `fraiseql-cli compile`
- Emit the same schema JSON as the Python and TypeScript official SDKs (cross-SDK parity tests)
- Pass the golden fixture tests in `tests/fixtures/golden/`
