#!/usr/bin/env bash
# test-check-no-runtime-copy.sh — САМОТЕСТ стража `check-no-runtime-copy.sh`.
#
# Почему самотест обязателен (требование владельца, план 231 трек Ж): механизм
# принуждения без собственного теста — это доверие на слово. Страж, который
# молча ничего не ловит, ХУЖЕ отсутствия стража: он создаёт ложное ощущение
# защиты. Поэтому каждый страж обязан доказать ОБА свойства:
#   (1) ЛОВИТ нарушение (не пропускает);
#   (2) НЕ ловит законный случай (нет ложных срабатываний).
#
# Первый самотест в scripts/selftest/ — образец формы для остальных стражей
# (gate.sh, arch-ratchet, guard-git, guard-memory, pre-commit, D-uniqueness).
#
# Запуск: scripts/selftest/test-check-no-runtime-copy.sh
# Выход: 0 — страж исправен, 1 — страж сломан (печатает, какое свойство упало).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/selftest/ — корень репы на три уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/check-no-runtime-copy.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
check() { # имя, ожидаемый_код, фактический_код
    if [ "$2" -eq "$3" ]; then
        echo "  ok: $1"
    else
        echo "  ПРОВАЛ: $1 — ожидался код $2, получен $3" >&2
        fails=$((fails + 1))
    fi
}

echo "самотест check-no-runtime-copy:"

# (1) ЛОВИТ: пакетная репа с копией рантайма — код 1.
mkdir -p "$tmp/fake-pkg/compiler-codegen/nova_rt"
touch "$tmp/fake-pkg/compiler-codegen/nova_rt/effects.h"
"$GUARD" "$tmp/fake-pkg" >/dev/null 2>&1
check "ловит копию рантайма в пакетной репе" 1 $?

# (2) НЕ ловит: главная репа (у неё есть compiler-codegen/Cargo.toml) — код 0.
mkdir -p "$tmp/fake-main/compiler-codegen/nova_rt"
touch "$tmp/fake-main/compiler-codegen/Cargo.toml"
"$GUARD" "$tmp/fake-main" >/dev/null 2>&1
check "НЕ ловит главную репу (дом рантайма законен)" 0 $?

# (3) НЕ ловит: обычная пакетная репа без копии — код 0.
mkdir -p "$tmp/clean-pkg/src"
"$GUARD" "$tmp/clean-pkg" >/dev/null 2>&1
check "НЕ ловит чистую пакетную репу" 0 $?

# (4) ЛОВИТ среди нескольких: одна грязная в наборе чистых — код 1.
"$GUARD" "$tmp/clean-pkg" "$tmp/fake-pkg" "$tmp/fake-main" >/dev/null 2>&1
check "ловит одну грязную среди чистых" 1 $?

# (5) Реальная главная репа не считается нарушением (страж не ломает себя).
"$GUARD" "$REPO_ROOT" >/dev/null 2>&1
check "НЕ ловит настоящую репу nova" 0 $?

if [ "$fails" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $fails свойств(а) стража не выполняются" >&2
    exit 1
fi
echo "самотест ok: страж ловит нарушения и не даёт ложных срабатываний"
