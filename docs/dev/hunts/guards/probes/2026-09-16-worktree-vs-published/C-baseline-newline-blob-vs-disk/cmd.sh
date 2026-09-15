#!/usr/bin/env bash
# Probe C -- check-baseline-newline.sh takes its LIST from git (tracked paths)
# and its CONTENT from the disk. The damage it exists to prevent happens to
# whoever READS the baseline out of the commit (CI, a clone), so the two
# objects have to agree -- and nothing makes them.
set -u
REPO="${NOVA_REPO:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")}"
G="$REPO/scripts/guards/check-baseline-newline.sh"
[ -f "$G" ] || { echo "no guard at $G"; exit 2; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
GC="git -c user.name=probe -c user.email=probe@example.com -C $T"

mkdir -p "$T/scripts/guards"
$GC init -q .
# committed WITHOUT a trailing newline -- the broken state the guard is for
printf 'rows=10\nmax=99' > "$T/scripts/guards/demo.baseline"
$GC add scripts/guards/demo.baseline
$GC commit -q -m init
echo "=== committed blob, last byte: ==="
$GC show HEAD:scripts/guards/demo.baseline | od -An -c | tail -2

echo
echo "=== state 1: working copy == commit (both broken) ==="
bash "$G" "$T" "$T"; echo "exit=$?"

echo
echo "=== state 2: newline added ONLY in the working copy (not git add-ed) ==="
printf 'rows=10\nmax=99\n' > "$T/scripts/guards/demo.baseline"
echo "--- commit still ends without a newline:"
$GC show HEAD:scripts/guards/demo.baseline | od -An -c | tail -1
bash "$G" "$T" "$T"; echo "exit=$?"
echo "    ^ green. On a clone, 'while read' still drops the last line of this baseline."

echo
echo "=== state 3: baseline tracked in the commit but absent from the disk ==="
rm "$T/scripts/guards/demo.baseline"
bash "$G" "$T" "$T"; echo "exit=$?"
echo "    ^ the published baseline is simply not judged (silently skipped)."
