#!/usr/bin/env bash
# Restore the Tus interop driver's dependencies from the committed lockfile.
#
# `crates/fraiseql-storage/tests/tus_interop.rs` drives FraiseQL's Tus endpoints
# with `tus-js-client`, the reference implementation. This installs it; the CI
# storage leg runs this immediately before the suite and sets TUS_INTEROP=1, the
# marker that turns the suite from "skip" into "run and fail loudly".
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

if [[ ! -d node_modules/tus-js-client ]]; then
  echo "→ installing tus interop deps (npm ci)"
  npm ci --no-audit --no-fund
fi

node -e "import('tus-js-client').then(m => console.log('tus-js-client', require('./package.json').dependencies['tus-js-client']))" 2>/dev/null \
  || node -e "console.log('tus-js-client', JSON.parse(require('fs').readFileSync('package.json')).dependencies['tus-js-client'])"
