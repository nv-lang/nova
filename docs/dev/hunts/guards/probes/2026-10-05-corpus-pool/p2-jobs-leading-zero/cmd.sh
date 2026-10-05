#!/bin/sh
# p2: NOVAC_POOL_JOBS with a leading zero passes novac_pool_jobs validation
# (only digits checked), but $(( ... % 08 )) is octal in sh arithmetic.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/tmp"
M="$W/root"; mkdir -p "$M/scripts/guards/lib" "$M/novac/fixtures"
cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
cp "$ROOT/scripts/guards/check-novac-no-panic.sh" "$M/scripts/guards/" || exit 2
for k in 1 2 3 4 5; do echo x > "$M/novac/fixtures/pos_n$k.nv"; done
printf '#!/bin/sh\nexit 0\n' > "$W/bin.sh"; chmod +x "$W/bin.sh"
for J in 8 08 09 010 abc -1 0 ""; do
  NOVAC_POOL_JOBS=$J TMPDIR="$W/tmp" sh "$M/scripts/guards/check-novac-no-panic.sh" "$M" "$W/bin.sh" > "$W/out" 2>&1; rc=$?
  echo "NOVAC_POOL_JOBS='$J' rc=$rc"; sed 's/^/    /' "$W/out" | grep -v "value too great" | head -8
done
