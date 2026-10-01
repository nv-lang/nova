#!/bin/sh
# Registry #1549 -- a const whose initialiser reads another const, in a module other
# than the entry one, is emitted to C with the BARE name of the const it reads:
# `static const nova_int nv_WEEK_MS = 7 * (DAY_MS)` while the definition is
# `nv_DAY_MS` (the C-name door escapes an ALL-CAPS name, #1440/#1446). Carrier:
# claude-limits 92fa204 (`src/storage/backup.nv`: `export const WEEK_MS = 7 * DAY_MS`).
#
# Run from the repository root:  sh docs/plans/repro/1549-const-ref-const-nonentry/cmd.sh
# Measured 2026-10-01:
#   main 0c024c5de:  CC-FAIL "use of undeclared identifier 'DAY_MS'"
#   after the fix:   "604800000 86400000"
# The same-named `DAY_MS` of feed.nv does not matter: renamed, it fails the same.
# The three files must sit side by side (`import backup.{week}`, `import feed.{day}`).
set -e
D="$(mktemp -d)"
for f in backup feed main; do cp "$(dirname "$0")/$f.nv.txt" "$D/$f.nv"; done
"${NOVA:-./nova-cli/target/release/nova}" test "$D/main.nv"
