#!/bin/bash
# Regenerates every docs/backend/<crate>.md from its template and the workspace source.
# usage: backend/tools/docgen/regen_all.sh [outdir]   (default: docs/backend)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
BACKEND=$(cd "$HERE/../.." && pwd)
OUT=$(mkdir -p "${1:-$BACKEND/../docs/backend}" && cd "${1:-$BACKEND/../docs/backend}" && pwd)
status=0
for d in domain domain-types store blob integrations vision knowledge agent pipeline api worker; do
  case $d in
    domain-types) src=$BACKEND/crates/domain/src;;
    api|worker) src=$BACKEND/bins/$d/src;;
    *) src=$BACKEND/crates/$d/src;;
  esac
  python3 "$HERE/docgen.py" "$src" "$HERE/$d.md.tpl" "$OUT/$d.md" >/dev/null || { echo "FAIL $d"; status=1; }
done
exit $status
