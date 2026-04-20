#!/usr/bin/env bash
# Extract module-level //! doc comments from every .rs file under src/
# and write them to docs/module_docs.txt, one block per file.
set -euo pipefail

cd "$(dirname "$0")/.."
out="docs/module_docs.txt"
mkdir -p "$(dirname "$out")"
: >"$out"

while IFS= read -r -d '' file; do
    rel="${file#./}"
    doc=$(awk '
        /^\/\/!/ { sub(/^\/\/![ ]?/, ""); print; next }
        NF { exit }
    ' "$file")
    {
        echo "=== $rel ==="
        if [[ -n "$doc" ]]; then echo "$doc"; else echo "(no //! doc)"; fi
        echo
    } >>"$out"
done < <(find src -name "*.rs" -print0 | sort -z)

echo "Wrote $(grep -c '^=== ' "$out") entries to $out"
