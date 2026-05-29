# Regenerate the typed client from the compiled schema, then type-check the example.
#
# Uses `cargo run -p fraiseql-cli --` so it works from a repo checkout without
# installing the `fraiseql` binary. If you have `fraiseql` on your PATH, replace
# FRAISEQL with `fraiseql`.

FRAISEQL ?= cargo run --quiet -p fraiseql-cli --

.PHONY: codegen typecheck clean

codegen:
	$(FRAISEQL) generate-client typescript --schema schema.compiled.json --out src/generated --force

typecheck:
	npx tsc --noEmit

clean:
	rm -rf src/generated
