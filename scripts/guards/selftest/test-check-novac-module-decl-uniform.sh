#!/usr/bin/env bash
# scripts/guards/selftest/test-check-novac-module-decl-uniform.sh — самотест стража
# «папка novac/src объявляет один модуль во всех файлах».
#
# ПОЧЕМУ. 2026-09-05 папка emit_c/ жила расколотой (6 `module emit_c` против
# 4 `module novac.emit_c`); модульные тесты входили списком и не видели, форма
# каталога падала E_D78. Самотест держит обе стороны: раскол — красный с именем
# папки и обоих написаний; единообразие — зелёный; чужая форма — красный; файл
# без module — красный; пустой каталог — красный (потеря мишени). Число случаев
# печатает счётчик, не рука.
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-module-decl-uniform.py"
T="${TMPDIR:-/tmp}/novac-module-decl-uniform.$$"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1" >&2; FAILED=$((FAILED+1)); }
run() { python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"; }
mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

mk() { mkdir -p "$T/$1/$2"; printf '%s\nfn f() -> int => 1\n' "$3" > "$T/$1/$2/$4"; }

# --- 1. единообразная папка — зелёный -----------------------------------------
mk uni sem "module novac.sem" a.nv; mk uni sem "module novac.sem" b.nv; mk uni sem "module novac.sem" c_test.nv
if run "$T/uni"; then ok "одно объявление на папку — зелёный"; else bad "единообразная папка покраснела: $(cat "$T/err")"; fi

# --- 2. ГЛАВНЫЙ случай: раскол ------------------------------------------------
mk split emit_c "module novac.emit_c" a.nv; mk split emit_c "module emit_c" b.nv; mk split emit_c "module emit_c" c.nv
if run "$T/split"; then
    bad "расколотая папка прошла зелёной"
else
    grep -q "emit_c" "$T/err" && grep -q "module emit_c" "$T/err" && ok "раскол пойман, названы папка и второе написание" || bad "красный, но без имени папки/написания"
fi

# --- 3. одно объявление, но чужой формы ---------------------------------------
mk short lex "module lex" a.nv; mk short lex "module lex" b.nv
if run "$T/short"; then bad "короткая форма прошла"; else grep -q "novac.lex" "$T/err" && ok "чужая форма красная, требуемая названа" || bad "красный, но требуемая форма не названа"; fi

# --- 4. файл без строки module ------------------------------------------------
mkdir -p "$T/nomod/parse"; printf 'fn f() -> int => 1\n' > "$T/nomod/parse/a.nv"; printf 'module novac.parse\n' > "$T/nomod/parse/b.nv"
if run "$T/nomod"; then bad "файл без module прошёл"; else ok "файл без module — красный"; fi

# --- 5. МИШЕНЬ ПОТЕРЯНА: каталог без папок с .nv --------------------------------
mkdir -p "$T/none/only_c"; printf 'int x;\n' > "$T/none/only_c/a.c"
if run "$T/none"; then bad "ноль папок — а страж зелёный"; else grep -q "мишень" "$T/err" && ok "ноль папок — красный, назван потерей мишени" || bad "красный, но не про мишень"; fi

# --- 6. живое дерево — зелёное после унификации 2026-09-05 ---------------------
if run "$ROOT/novac/src"; then ok "живое дерево единообразно"; else bad "живое дерево красное: $(head -3 "$T/err")"; fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-novac-module-decl-uniform ok: $CASES/$CASES — раскол, чужая форма, файл без module и потеря мишени краснеют; единообразие и живое дерево зелены"
    exit 0
fi
exit 1
