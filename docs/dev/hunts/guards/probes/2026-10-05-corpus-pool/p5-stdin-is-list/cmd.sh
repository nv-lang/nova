#!/bin/sh
# p5: differential stage 2. one_behaviour calls `bash "$SMOKE" ...` without
# </dev/null, so the smoke (and the fixture programs it runs: smoke lines
# 225-226 run "$ORACLE_EXE" and emitted_prog.exe without </dev/null) inherit
# the pool thread's stdin = the fixture LIST. The stub smoke stands in for a
# fixture program that reads stdin (cat) on pos_n2 only.
# Expected: 7 fixtures judged. J=1 and J=3.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/tmp"
M="$W/root"; mkdir -p "$M/scripts/guards/lib" "$M/scripts/tools" "$M/novac/fixtures" "$M/nova-cli/target/release"
cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
cp "$ROOT/scripts/guards/check-novac-differential.sh" "$M/scripts/guards/" || exit 2
for k in 1 2 3 4 5 6 7; do echo x > "$M/novac/fixtures/pos_n$k.nv"; done
printf '#!/bin/sh\nexit 0\n' > "$M/nova-cli/target/release/nova.exe"; chmod +x "$M/nova-cli/target/release/nova.exe"
printf '#!/bin/sh\nexit 0\n' > "$W/bin.sh"; chmod +x "$W/bin.sh"
printf '#!/bin/sh\n[ "$1" = --prepare ] && exit 0\ncase "$1" in *pos_n2.nv) cat > "%s/stdin-seen.txt";; esac\nexit 0\n' "$W" > "$M/scripts/tools/novac-e1-smoke.sh"
G="$M/scripts/guards/check-novac-differential.sh"
export TMPDIR="$W/tmp" NOVAC_DIFF_TIER=push
for J in 1 3; do
  rm -f "$W/stdin-seen.txt"
  NOVAC_POOL_JOBS=$J sh "$G" "$M" "$W/bin.sh" > "$W/out.$J" 2>&1; echo "== J=$J rc=$? =="; cat "$W/out.$J"
  echo "-- what the 'fixture program' on pos_n2 read from stdin:"; [ -f "$W/stdin-seen.txt" ] && sed 's|.*/novac/|  .../novac/|' "$W/stdin-seen.txt" || echo "  NO stdin-seen.txt - probe broken"
done
