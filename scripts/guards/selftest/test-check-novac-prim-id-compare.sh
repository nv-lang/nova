#!/usr/bin/env bash
# scripts/guards/selftest/test-check-novac-prim-id-compare.sh — самотест стража
# «семья прима спрашивается у двери, не у id» (№910).
#
# ЧЕТЫРЕ СЛУЧАЯ, каждый отвечает на свой вопрос:
#   1. подложка на базе (count равен базе) — зелёный;
#   2. одно сравнение сверх базы — КРАСНЫЙ, и место названо файл:строка;
#   3. ноль файлов .nv под судом — КРАСНЫЙ как потеря мишени, а не зелёный ноль
#      (урок охоты guards x К7 2026-09-04);
#   4. сравнение в комментарии — НЕ считается (проза — не код).
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-prim-id-compare.py"
T="${TMPDIR:-/tmp}/novac-prim-id-selftest.$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }
mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

mk_src() {  # $1 = dir, $2 = number of comparisons to plant
    mkdir -p "$1/check" "$1/emit_c"
    { echo 'module p'
      echo 'fn f(t TyId) -> bool {'
      for _ in $(seq 1 "$2"); do echo '    if t == @ctx.prims.int_id { return true }'; done
      echo '    false'
      echo '}'; } > "$1/check/a.nv"
    printf 'module q\nfn g() -> int => 1\n' > "$1/emit_c/b.nv"
}
run() { NOVAC_PRIM_ID_BASELINE="$2" python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"; }

printf 'count=2\n' > "$T/base2"

# --- 1. на базе -----------------------------------------------------------------
mk_src "$T/at" 2
if run "$T/at" "$T/base2"; then ok "два сравнения при базе 2 — зелёный"; else bad "на базе, а красный: $(cat "$T/err")"; fi

# --- 2. рост --------------------------------------------------------------------
mk_src "$T/grow" 3
if run "$T/grow" "$T/base2"; then bad "три сравнения при базе 2 прошли зелёным"; else
    grep -q "check/a.nv:" "$T/err" && ok "рост красный, место названо" || bad "красный, но без адреса: $(cat "$T/err")"; fi

# --- 3. мишень потеряна ----------------------------------------------------------
mkdir -p "$T/none/check" "$T/none/emit_c"
if run "$T/none" "$T/base2"; then bad "ноль файлов под судом — а страж зелёный"; else
    grep -q "мишень" "$T/err" && ok "ноль файлов — красный, назван потерей мишени" || bad "красный, но не про мишень"; fi

# --- 4. проза не считается -------------------------------------------------------
mk_src "$T/prose" 2
echo '    // the old way was t == @ctx.prims.int_id and it is gone' >> "$T/prose/check/a.nv"
if run "$T/prose" "$T/base2"; then ok "сравнение в комментарии не считается"; else bad "комментарий посчитан как код: $(cat "$T/err")"; fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-novac-prim-id-compare ok: рост и потеря мишени краснеют, база и проза законны"
    exit 0
fi
exit 1
