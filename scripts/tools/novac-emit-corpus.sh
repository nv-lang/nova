#!/bin/sh
# Emit the whole differential corpus with novac into one directory, for a
# byte-for-byte C comparison across a wave (plan 274.8 M1 criterion 6).
# Usage: sh scratch/emit_corpus.sh <outdir>   (run from the worktree root)
# Corpus: examples/**/*.nv (novac-diff-corpus default) + novac/fixtures/**/pos_*.nv.
set -u
OUT="$1"
NOVAC="novac/target/novac.exe"
mkdir -p "$OUT"
n=0; ok=0
for f in $(find examples novac/fixtures -name '*.nv' | grep -E 'examples/|/pos_[0-9]+\.nv$' | sort); do
    n=$((n+1))
    o="$OUT/$(echo "$f" | tr '/' '_').c"
    if "$NOVAC" emit "$f" > "$o" 2>"$o.err"; then ok=$((ok+1)); else echo "rc=$? $f" >> "$OUT/_refused.txt"; fi
done
echo "emit_corpus: files $n, emitted ok $ok, out $OUT"
