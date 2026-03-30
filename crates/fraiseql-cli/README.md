# fraiseql-cli

CLI tools for FraiseQL v2. This binary crate provides schema compilation, database utilities, and development tooling for building and maintaining FraiseQL projects.

## Features

- Schema compilation: `schema.json` + `fraiseql.toml` to `schema.compiled.json`
- Schema validation and linting
- Database introspection and migration generation
- Query cost analysis and dependency graphing
- SBOM generation for supply chain auditing
- Trusted document validation
- MCP server integration via `FRAISEQL_MCP_STDIO` environment variable

## Installation

```sh
cargo install fraiseql-cli
```

## Usage

```sh
fraiseql-cli compile schema.json
fraiseql-cli validate-documents manifest.json
fraiseql-cli introspect --database-url postgres://localhost/mydb
```

## Documentation

- [API Documentation](https://docs.rs/fraiseql-cli)
- [FraiseQL Documentation](https://docs.fraiseql.dev)
- [Repository](https://github.com/fraiseql/fraiseql)

## License

MIT OR Apache-2.0
