#!/usr/bin/env bash
# Probe B -- check-script-eol-pinned.py judges "what the next checkout will
# deliver", but reads BOTH of its inputs from the working copy:
#   * the file list  -- os.walk over scripts/ and .claude/
#   * the eol attribute -- git check-attr, which reads the WORKING-TREE
#     .gitattributes, not the committed one.
set -u
REPO="${NOVA_REPO:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$PWD")}"
G="$REPO/scripts/guards/check-script-eol-pinned.py"
[ -f "$G" ] || { echo "no guard at $G"; exit 2; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
GC="git -c user.name=probe -c user.email=probe@example.com -C $T"

mkdir -p "$T/scripts" "$T/.claude"
printf '#!/bin/sh\necho hi\n' > "$T/scripts/tool.sh"
printf 'x\n' > "$T/.claude/note.md"
# committed .gitattributes does NOT pin *.sh
printf '*.txt text\n' > "$T/.gitattributes"
$GC init -q .
$GC add .gitattributes scripts/tool.sh .claude/note.md
$GC commit -q -m init

echo "=== state 1: nothing pinned, neither in tree nor in commit ==="
python "$G" "$T"; echo "exit=$?"

echo
echo "=== state 2: pin added ONLY in the working copy (NOT git add-ed) ==="
printf '*.txt text\n*.sh text eol=lf\n' > "$T/.gitattributes"
echo "--- what the commit still says (git show HEAD:.gitattributes):"
$GC show HEAD:.gitattributes | sed 's/^/    /'
echo "--- what a fresh clone would get for scripts/tool.sh:"
$GC ls-tree -r --name-only HEAD | sed 's/^/    /'
python "$G" "$T"; echo "exit=$?"
echo "    ^ green, while the PUBLISHED .gitattributes pins nothing:"
echo "      a fresh checkout with core.autocrlf=true hands scripts/tool.sh over as CRLF."
