#!/bin/sh
# run from the nova worktree root. Emits the whole differential corpus (examples/**/*.nv +
# novac/fixtures/**/pos_*.nv) with novac and counts, among the files that were EMITTED (not
# refused by the subset), how many carry each control-flow form. On 2026-09-06 before the fixture
# novac/fixtures/control_flow/pos_1.nv: 92 emitted, `else` on a plain if statement -- 0, `else if`
# -- 0, while -- 1, range for -- 1: the forms M2b-1a moved had no carrier under the byte-identity
# check, which is why a broken `else` spelling moved 0 files while a broken `if` spelling moved 6.
P=docs/dev/hunts/novac/probes/2026-09-06-emitc-k2/p02-else-carrier-missing
OUT="${TMPDIR:-/tmp}/p02-corpus"
rm -rf "$OUT"; mkdir -p "$OUT"
find examples novac/fixtures -name '*.nv' | grep -E 'examples/|/pos_[0-9]+\.nv$' | sort > "$OUT/list"
n=0; ok=0
while IFS= read -r f; do
    n=$((n+1))
    o="$OUT/$(echo "$f" | tr '/' '_').c"
    if novac/target/novac.exe emit "$f" > "$o" 2>"$o.err"; then ok=$((ok+1)); else echo "rc=$? $f" >> "$OUT/_refused.txt"; fi
done < "$OUT/list"
echo "emitted $ok of $n"
python "$P/census.py" "$OUT" < "$OUT/list"
