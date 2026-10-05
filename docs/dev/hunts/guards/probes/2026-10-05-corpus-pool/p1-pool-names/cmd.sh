#!/bin/sh
# p1: no-panic pool over names with spaces; J=1, J=3, J=50 (> N).
# Fake novac logs every call; each fixture must be checked exactly once.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/tmp"
M="$W/root"; mkdir -p "$M/scripts/guards/lib" "$M/novac/fixtures/sub dir"
cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
cp "$ROOT/scripts/guards/check-novac-no-panic.sh" "$M/scripts/guards/" || exit 2
F="$M/novac/fixtures"
for n in "pos a.nv" "pos  b.nv" " lead.nv" "trail .nv" "sub dir/pos c.nv" "pos_7.nv"; do echo x > "$F/$n"; done
N=$(find "$F" -type f -name '*.nv' | wc -l | tr -d ' ')
echo "fixtures on disk: $N"
printf '#!/bin/sh\nprintf "%%s\n" "$2" >> "%s/calls.log"\ncase "$2" in *"pos  b.nv") [ -n "$PANIC" ] && exit 101;; esac\nexit 0\n' "$W" > "$W/bin.sh"; chmod +x "$W/bin.sh"
for J in 1 3 50; do
  rm -f "$W/calls.log"
  NOVAC_POOL_JOBS=$J TMPDIR="$W/tmp" sh "$M/scripts/guards/check-novac-no-panic.sh" "$M" "$W/bin.sh" > "$W/out.$J" 2>&1; rc=$?
  [ -f "$W/calls.log" ] || { echo "J=$J: NO calls.log - probe broken"; continue; }
  calls=$(wc -l < "$W/calls.log" | tr -d ' '); dups=$(sort "$W/calls.log" | uniq -d | wc -l | tr -d ' ')
  echo "J=$J rc=$rc calls=$calls dup-lines=$dups :: $(tail -n 1 "$W/out.$J")"
done
rm -f "$W/calls.log"
PANIC=1 NOVAC_POOL_JOBS=3 TMPDIR="$W/tmp" sh "$M/scripts/guards/check-novac-no-panic.sh" "$M" "$W/bin.sh" > "$W/out.panic" 2>&1; rc=$?
echo "panic on 'pos  b.nv', J=3: rc=$rc"; cat "$W/out.panic" | head -3
