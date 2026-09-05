#!/usr/bin/env bash
# Проба к находке: arch-ratchet объявляет
#   «Две метрики emit_c.rs НЕ МОГУТ РАСТИ относительно baseline»
#   «Выход: 0 — метрики <= baseline, 1 — рост без правки baseline»
# Код же берёт файл по пути, ОТНОСИТЕЛЬНОМУ ТЕКУЩЕМУ КАТАЛОГУ
#   EMIT="compiler-codegen/src/codegen/emit_c.rs"
# и не проверяет, что файл нашёлся. Мишени нет — метрики пустая и ноль,
# обе «<= baseline», вердикт ok, код 0.
#
# Страж не принимает ни корня аргументом, ни переменной окружения: единственный
# шов — cwd. Поэтому проба запускает его из ПУСТОГО каталога, ничего в рабочем
# дереве не трогая.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/arch-ratchet.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/arch-ratchet.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0

echo "--- artefact check: cwd is an empty dir, the target file is absent ---"
ls -a "$TMP"
if [ -e "$TMP/compiler-codegen/src/codegen/emit_c.rs" ]; then
    echo "TARGET PRESENT - probe invalid" >&2
    exit 99
fi
echo "  confirmed: compiler-codegen/src/codegen/emit_c.rs is NOT under cwd"

echo "--- guard (baseline read from its own directory: the real one) ---"
( cd "$TMP" && bash "$GUARD" )
rc=$?
echo "RC=$rc"
exit "$rc"
