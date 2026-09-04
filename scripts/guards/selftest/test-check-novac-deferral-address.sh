#!/bin/sh
# scripts/guards/selftest/test-check-novac-deferral-address.sh — самотест стража
# «отложенная проверка называет АДРЕС получателя» (конвенции novac П6, реестр №921).
#
# ШЕСТЬ СЛУЧАЕВ, каждый отвечает на свой вопрос:
#   1. отсылка С адресом (тем же и соседней строкой комментария) — ЗЕЛЁНЫЙ;
#   2. отсылка БЕЗ адреса — КРАСНЫЙ, и в тексте отказа назван адрес места
#      (файл:строка) — иначе отказ не приводит к правке;
#   3. адрес через строку — КРАСНЫЙ: окно узкое намеренно, адрес абзацем ниже
#      читателю строки не виден;
#   4. файлы есть, отсылок НЕТ — КРАСНЫЙ как ПОТЕРЯ МИШЕНИ, а не зелёный ноль;
#   5. каталог без единого .nv — КРАСНЫЙ по той же причине;
#   6. базы нет — КРАСНЫЙ: храповику нечем судить.
#
# Самотест не зависит от настоящего дерева: только свои фикстуры.
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-deferral-address.py"
T="${TMPDIR:-/tmp}/novac-deferral-address-selftest.$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' EXIT

run() {  # $1 = каталог-мишень, $2 = файл базы
    NOVAC_DEFERRAL_ADDRESS_BASELINE="$2" python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"
}

printf 'unaddressed=0\n' > "$T/base0"

# --- 1. отсылка с адресом ---------------------------------------------------------
mkdir -p "$T/clean/check"
{ echo 'module p'
  echo '// arity is checked by `bind_ctx.check_arity()` -- the door that refuses it'
  echo 'fn f() -> int => 1'
  echo '// the shape is judged by the subset walk'
  echo '// -- it stands at novac/src/pipeline/subset.nv:120'
  echo 'fn g() -> int => 2'; } > "$T/clean/check/a.nv"
if run "$T/clean" "$T/base0"; then
    ok "две отсылки, обе с адресом (своя строка и соседняя) — зелёный"
else
    bad "чистый вход, а красный: $(cat "$T/err")"
fi

# --- 2. отсылка без адреса --------------------------------------------------------
mkdir -p "$T/dirty/check"
{ echo 'module p'
  echo '// arity is checked by `bind_ctx.check_arity()` -- the door that refuses it'
  echo 'fn f() -> int => 1'
  echo '// the rest is reported by somebody else, somewhere'
  echo 'fn g() -> int => 2'; } > "$T/dirty/check/a.nv"
if run "$T/dirty" "$T/base0"; then
    bad "отсылка без адреса прошла зелёным"
else
    if grep -q 'check/a.nv:4' "$T/err"; then
        ok "безадресная отсылка красная, место названо файл:строка"
    else
        bad "красный, но без адреса места: $(cat "$T/err")"
    fi
fi

# --- 3. адрес через строку — не считается ------------------------------------------
mkdir -p "$T/far/check"
{ echo 'module p'
  echo '// the rest is reported by somebody else'
  echo '//'
  echo '// exactly here: novac/src/sem/defs.nv:196'
  echo 'fn g() -> int => 2'; } > "$T/far/check/a.nv"
if run "$T/far" "$T/base0"; then
    bad "адрес через строку засчитан — окно шире трёх строк"
else
    grep -q 'check/a.nv:2' "$T/err" \
        && ok "адрес через строку не засчитан: окно ровно три строки" \
        || bad "красный, но не на той строке: $(cat "$T/err")"
fi

# --- 4. мишень потеряна: файлы есть, отсылок нет ------------------------------------
mkdir -p "$T/nohit/check"
{ echo 'module p'
  echo '// plain prose with no deferral at all'
  echo 'fn f() -> int => 1'; } > "$T/nohit/check/a.nv"
if run "$T/nohit" "$T/base0"; then
    bad "ноль отсылок — а страж зелёный (это и есть тихий зелёный ноль)"
else
    grep -q "$(printf '\320\274\320\270\321\210\320\265\320\275\321\214')" "$T/err" \
        && ok "ноль отсылок — красный, назван потерей мишени" \
        || bad "красный, но не про потерю мишени: $(cat "$T/err")"
fi

# --- 5. мишень потеряна: ни одного .nv ---------------------------------------------
mkdir -p "$T/none/check"
if run "$T/none" "$T/base0"; then
    bad "каталог без .nv — а страж зелёный"
else
    grep -q "$(printf '\320\274\320\270\321\210\320\265\320\275\321\214')" "$T/err" \
        && ok "каталог без .nv — красный, назван потерей мишени" \
        || bad "красный, но не про потерю мишени: $(cat "$T/err")"
fi

# --- 6. базы нет --------------------------------------------------------------------
if run "$T/dirty" "$T/base-missing"; then
    bad "базы нет — а страж зелёный"
else
    grep -q 'unaddressed=N' "$T/err" \
        && ok "базы нет — красный, сказано чем судить" \
        || bad "красный, но молчит про базу: $(cat "$T/err")"
fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-novac-deferral-address ok: адрес в окне трёх строк принят, безадресная отсылка и потеря мишени краснеют с названным местом"
    exit 0
fi
exit 1
