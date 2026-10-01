#!/bin/sh
# Probe self-build-token: the carrier in Carina's OWN source. novac.lex declares
# `export type Token { kind, leading, text }` (novac/src/lex/lex.nv); std declares
# a PRIVATE `type Token enum` (std/src/encoding/json.nv). In the unit self-build
# (NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src, the measure 0.2 of 274.11) the parse
# module's three `Token { kind: .. }` constructions are refused as naming fields
# the type does not declare (#812) -- the handed `Token` is json's enum.
PROBE="$(cd "$(dirname "$0")" && pwd)"
d="$PROBE"
if [ -z "$ROOT" ]; then
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    ROOT="$d"
fi
[ -n "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac.exe"
cd "$ROOT" || exit 1
echo "=== the two declarations"
grep -n "^export type Token \|^type Token " novac/src/lex/lex.nv std/src/encoding/json.nv
echo "=== the constructions in parse.nv"
grep -n "Token { kind" novac/src/parse/parse.nv
echo "=== SUBJECT: NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src novac check novac/src/parse/*.nv -- the #812 lines"
NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src "$NOVAC" check novac/src/parse/*.nv > "${TMPDIR:-/tmp}/sbt.$$" 2>&1
echo "novac check rc=$?"
grep "#812" "${TMPDIR:-/tmp}/sbt.$$" | sed 's/"message".*//'
# byte offset -> line, for each #812 diagnostic
grep "#812" "${TMPDIR:-/tmp}/sbt.$$" | sed 's/.*"start":\([0-9]*\).*/\1/' | while read -r off; do
    printf 'offset %s -> parse.nv line %s\n' "$off" "$(head -c "$off" novac/src/parse/parse.nv | wc -l | awk '{print $1 + 1}')"
done
rm -f "${TMPDIR:-/tmp}/sbt.$$"
