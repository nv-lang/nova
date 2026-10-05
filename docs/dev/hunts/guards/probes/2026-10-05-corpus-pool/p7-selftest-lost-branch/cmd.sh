#!/bin/sh
# p7: do the two selftests have a RED case for the promise "a fixture without
# a result file is red, not a silent skip"? Copies of both guards get the
# no-result branch disabled ONE AT A TIME (if [ ! -f ...]; then -> if false; then),
# and the guard's own selftest (copied beside it) is run against each mutant.
# Expected: each mutant reddens its selftest.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"
mk() { # $1 = mutant name
  M="$W/$1"; mkdir -p "$M/scripts/guards/lib" "$M/scripts/guards/selftest"
  cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
  for g in check-novac-differential check-novac-no-panic; do
    cp "$ROOT/scripts/guards/$g.sh" "$M/scripts/guards/" || exit 2
    cp "$ROOT/scripts/guards/selftest/test-$g.sh" "$M/scripts/guards/selftest/" || exit 2
  done
}
mut() { # $1 mutant dir, $2 guard, $3 file suffix
  f="$W/$1/scripts/guards/$2.sh"
  before=$(grep -c "if \[ ! -f \"\$T/r/\$i.$3\" \]; then" "$f")
  sed -i "s|if \[ ! -f \"\$T/r/\$i.$3\" \]; then|if false; then|" "$f"
  after=$(grep -c 'if false; then' "$f")
  echo "mutant $1: $2 branch .$3 replaced: before=$before after=$after"
  [ "$before" = 1 ] && [ "$after" = 1 ] || { echo "  probe broken: mutation did not apply"; return 1; }
}
run() { # $1 mutant dir, $2 guard
  bash "$W/$1/scripts/guards/selftest/test-$2.sh" > "$W/$1.st.out" 2>&1; rc=$?
  echo "  selftest test-$2 on mutant: rc=$rc :: $(tail -n 1 "$W/$1.st.out")"
}
mk m0; echo "control (no mutation):"; run m0 check-novac-no-panic; run m0 check-novac-differential
mk m1; mut m1 check-novac-no-panic rc && run m1 check-novac-no-panic
mk m2; mut m2 check-novac-differential v && run m2 check-novac-differential
mk m3; mut m3 check-novac-differential b && run m3 check-novac-differential
