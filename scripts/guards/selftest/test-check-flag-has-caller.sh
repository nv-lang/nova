#!/usr/bin/env bash
# Самотест check-flag-has-caller.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-flag-has-caller.sh"
TMP="${TMPDIR:-/tmp}/selftest_flagcaller_$$"
BASE="$TMP/baseline"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {
    rm -rf "$TMP"
    mkdir -p "$TMP/compiler-codegen/src" "$TMP/scripts" "$TMP/docs" "$TMP/.github"
    : > "$TMP/AGENTS.md"
}
trap 'rm -rf "$TMP"' EXIT

# 1. Флаг читается кодом и взводится скриптом — зелено.
setup
printf 'let x = std::env::var("NOVA_WIRED").is_ok();\n' > "$TMP/compiler-codegen/src/a.rs"
printf 'NOVA_WIRED=1 bash something\n' > "$TMP/scripts/gate.sh"
echo 'silent_flags=0' > "$BASE"
out=$(NOVA_FLAGCALLER_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "флаг со скриптом-вызывающим проходит"; else bad "ложный отказ на взведённом флаге: $out"; fi

# 2. Флаг читается кодом и НИГДЕ больше — красно. Это и есть предмет стража:
#    снаружи такой флаг неотличим от несделанной работы (реестр №575).
setup
printf 'let x = std::env::var("NOVA_SILENT").is_ok();\n' > "$TMP/compiler-codegen/src/a.rs"
echo 'silent_flags=0' > "$BASE"
out=$(NOVA_FLAGCALLER_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "NOVA_SILENT"; then ok "ловит флаг без вызывающего"; else bad "не поймал безмолвный флаг (rc=$rc): $out"; fi

# 3. Описания в docs/ достаточно: у флага есть человек-вызывающий, и он знает,
#    что взводить. Страж не требует именно автоматики — он требует, чтобы о
#    флаге знал хоть кто-то вне читающего его кода.
setup
printf 'let x = std::env::var("NOVA_DOCUMENTED").is_ok();\n' > "$TMP/compiler-codegen/src/a.rs"
printf 'Переменная `NOVA_DOCUMENTED` включает то-то.\n' > "$TMP/docs/x.md"
echo 'silent_flags=0' > "$BASE"
out=$(NOVA_FLAGCALLER_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "описания в docs/ достаточно"; else bad "ложный отказ на описанном флаге: $out"; fi

# 4. Храповик: долг равный базе — зелено, превышение — красно.
setup
printf 'let a = std::env::var("NOVA_S1").is_ok();\nlet b = std::env::var("NOVA_S2").is_ok();\n' > "$TMP/compiler-codegen/src/a.rs"
echo 'silent_flags=2' > "$BASE"
out=$(NOVA_FLAGCALLER_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "равенство базе — не ложное срабатывание"; else bad "ложный отказ при равенстве базе: $out"; fi
echo 'silent_flags=1' > "$BASE"
out=$(NOVA_FLAGCALLER_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "ВЫРОСЛО"; then ok "ловит рост сверх базы"; else bad "не поймал рост (rc=$rc): $out"; fi

# 5. Снижение долга — зелено, но с указанием опустить базу: иначе храповик
#    молча теряет достигнутое.
echo 'silent_flags=5' > "$BASE"
out=$(NOVA_FLAGCALLER_BASELINE="$BASE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "СНИЗИЛСЯ"; then ok "сообщает о снижении долга"; else bad "не сообщил о снижении: $out"; fi

# 6. Страж назван на странице правил.
RULES="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/docs/dev/rules-for-agents.md"
if grep -q "check-flag-has-caller.sh" "$RULES" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-flag-has-caller: 7/7 ok"; exit 0; fi
echo "селфтест check-flag-has-caller: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
