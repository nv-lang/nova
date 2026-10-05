#!/bin/sh
# p6: differential, NOVAC_SMOKE unset (stage 2 ON), but
# <root>/scripts/tools/novac-e1-smoke.sh is absent (renamed/moved).
# Promise: every fixture gets exactly one stage-2 result; skipping a stage
# names itself (No.992). Expected: red or a named skip line.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/tmp"
M="$W/root"; mkdir -p "$M/scripts/guards/lib" "$M/scripts/tools" "$M/novac/fixtures" "$M/nova-cli/target/release"
cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
cp "$ROOT/scripts/guards/check-novac-differential.sh" "$M/scripts/guards/" || exit 2
for k in 1 2 3; do echo x > "$M/novac/fixtures/pos_n$k.nv"; done
printf '#!/bin/sh\nexit 0\n' > "$M/nova-cli/target/release/nova.exe"; chmod +x "$M/nova-cli/target/release/nova.exe"
printf '#!/bin/sh\nexit 0\n' > "$W/bin.sh"; chmod +x "$W/bin.sh"
[ -e "$M/scripts/tools/novac-e1-smoke.sh" ] && { echo "probe broken: smoke exists"; exit 2; }
unset NOVAC_SMOKE
export TMPDIR="$W/tmp"
echo "== tier push =="
NOVAC_DIFF_TIER=push sh "$M/scripts/guards/check-novac-differential.sh" "$M" "$W/bin.sh"; echo "rc=$?"
echo "== tier full, NOVAC_CORPUS=0 =="
NOVAC_CORPUS=0 sh "$M/scripts/guards/check-novac-differential.sh" "$M" "$W/bin.sh"; echo "rc=$?"
