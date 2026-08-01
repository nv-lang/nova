#!/bin/sh
# Самотест check-bug-number-sync.sh: (1) ловит новый маркер без №,
# (2) не ложнит на нумерованном, (3) baseline-исключение работает.
set -u
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-bug-number-sync.sh"
TMP="${TMPDIR:-/tmp}/bns_selftest_$$"
rm -rf "$TMP"; mkdir -p "$TMP/docs/plans" "$TMP/scripts/guards"
cp "$GUARD" "$TMP/scripts/guards/"
: > "$TMP/scripts/guards/bug-number-sync.baseline"
printf '| 1 | `[M-numbered-bug]` — text | OPEN |\n' > "$TMP/docs/plans/221.1-bug-sweep.md"
printf '| `[M-numbered-bug]` | text |\n' > "$TMP/docs/plans/backlog-followups.md"
sh "$TMP/scripts/guards/check-bug-number-sync.sh" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: ложняк на нумерованном"; rm -rf "$TMP"; exit 1; }
printf '| `[M-new-unnumbered-bug]` | text |\n' >> "$TMP/docs/plans/backlog-followups.md"
sh "$TMP/scripts/guards/check-bug-number-sync.sh" "$TMP" >/dev/null 2>&1 && { echo "SELFTEST FAIL: не поймал безномерный"; rm -rf "$TMP"; exit 1; }
printf 'M-new-unnumbered-bug\n' >> "$TMP/scripts/guards/bug-number-sync.baseline"
sh "$TMP/scripts/guards/check-bug-number-sync.sh" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: baseline-исключение не работает"; rm -rf "$TMP"; exit 1; }
rm -rf "$TMP"
echo "selftest check-bug-number-sync: OK (ловит безномерный / без ложняка / baseline работает)"
