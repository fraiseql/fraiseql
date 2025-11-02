# Extracted from: docs/core/postgresql-extensions.md
# Block number: 4
# FraiseQL checks for jsonb_ivm
if has_jsonb_ivm:
    # Use fast incremental merge
    sql = "UPDATE tv_user SET data = jsonb_merge_shallow(data, $1)"
else:
    # Fall back to full rebuild (slower but works)
    sql = "UPDATE tv_user SET data = $1"
