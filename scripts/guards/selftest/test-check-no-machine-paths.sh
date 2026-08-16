#!/usr/bin/env bash
# selftest для check-no-machine-paths.sh (№698).
# Доказывает: страж зелёный на дереве; КРАСНЫЙ на захардкоженном пути в
# исполняемой строке скрипта; НЕ ложнит на пути в комментарии, в строке
# «Проверялся:», в тексте шага гейта и в самотестах хуков.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-no-machine-paths.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

# Поддельный репозиторий: git init + один скрипт.
mk() { rm -rf "$TMP/r"; mkdir -p "$TMP/r/scripts/tools" "$TMP/r/scripts/claude-hooks/selftest"
       git -C "$TMP/r" init -q 2>/dev/null; }
addf() { printf '%s\n' "$2" > "$TMP/r/$1"; git -C "$TMP/r" add "$1" 2>/dev/null; }

echo "== проходит =="
out=$(bash "$G" "$ROOT" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'ok: скриптов проверено'; then ok "настоящее дерево — зелёный"; else bad "ложный красный на дереве: $out"; fi

mk; addf scripts/tools/x.sh '#!/bin/sh
# example: git -C d:/Sources/nv-lang/nova status
ROOT="$(pwd)"
echo ok'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "путь в комментарии — не считается"; else bad "ложный красный на комментарии: $out"; fi

mk; addf scripts/tools/x.sh '#!/bin/sh
step "рабочие деревья только в d:/Sources/nv-lang (№561)"'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "текст шага гейта — не считается"; else bad "ложный красный на тексте шага: $out"; fi

mk; addf scripts/claude-hooks/selftest/t.py 'CASES = [("git -C /d/Sources/nv-lang/nova status", False)]'
addf scripts/tools/clean.sh '#!/bin/sh
echo ok'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "тестовые данные самотеста хука — не считаются"; else bad "ложный красный на самотесте хука: $out"; fi

echo "== ловит =="
mk; addf scripts/tools/x.sh '#!/bin/sh
export NOVA_GC_LIB_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\lib"'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'x.sh'; then ok "Windows-путь в export — красный, файл назван"; else bad "пропустил D:\\Sources в export (rc=$rc): $out"; fi

mk; addf scripts/tools/x.sh '#!/bin/sh
MAIN_REPO="d:/Sources/nv-lang/nova"'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then ok "MAIN_REPO с путём — красный"; else bad "пропустил MAIN_REPO: $out"; fi

mk; addf scripts/tools/x.sh '#!/bin/sh
cp /mnt/d/Sources/nv-lang/nova/x .'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then ok "WSL-путь /mnt/d/Sources — красный"; else bad "пропустил /mnt/d/Sources: $out"; fi

mk; addf scripts/tools/x.py 'H = r"C:\Users\Someone\.claude\x"'
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then ok "C:\\Users\\<имя> в python — красный"; else bad "пропустил C:\\Users: $out"; fi

# Ноль без строки — не проверка (№645).
if grep -q '\$NAME ok:' "$G"; then ok "страж печатает свою строку ok:"; else bad "у стража нет строки ok:"; fi

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || { echo "selftest check-no-machine-paths: ПРОВАЛ" >&2; exit 1; }
echo "selftest check-no-machine-paths: OK (зелёный на дереве и на 3 законных формах / красный на 4 формах хардкода)"
exit 0
