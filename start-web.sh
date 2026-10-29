#!/usr/bin/env sh
set -eu
if ! command -v node >/dev/null 2>&1; then
  echo "Node.js is missing. Install the version specified in .node-version, then retry." >&2
  exit 1
fi
exec node "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/scripts/start-web.mjs" "$@"
