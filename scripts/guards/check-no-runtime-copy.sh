#!/usr/bin/env bash
# check-no-runtime-copy.sh — машинный страж против копий рантайма компилятора
# внутри пакетных реп и worktree'ов.
#
# ПОЧЕМУ (реестр 221.1 №138, урок 2026-07-27). Компилятор ищет рантайм
# (`compiler-codegen/nova_rt/`) относительно корня пакета. Исторически это
# «чинили» копированием каталога внутрь каждой репы/worktree. Копия:
#   * НЕ под git (git status чист) → её протухание НЕВИДИМО;
#   * ШАДОВИТ настоящий рантайм главной репы;
#   * при расхождении версий даёт невнятный CC-FAIL на СГЕНЕРИРОВАННОМ коде
#     (реальный случай: `unknown type name '_nova_main_fiber_co'` в nova-http —
#     копия заголовков была снята ДО фикса №108 «main как файбер», и полтора
#     часа диагностики ушло на поиск «регрессии компилятора», которой не было);
#   * весит сотни мегабайт на каждую репу (измерено: 526 МБ в nova-polaris,
#     331 МБ в одном worktree — суммарно больше гигабайта мусора).
#
# КАК ПРАВИЛЬНО. Копия НЕ НУЖНА: есть штатные env-переменные, сделанные
# симметрично `NOVA_STD_PATH` именно для сборки вне монорепы
# (nova-cli/src/main.rs, `env_path_override`):
#
#   export NOVA_RT_DIR="<главная-репа>/compiler-codegen/nova_rt"
#   export NOVA_CG_INCLUDE="<главная-репа>/compiler-codegen"
#   export NOVA_STD_PATH="<главная-репа>/std/src"
#   export NOVA_GC_LIB_DIR="<главная-репа>/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
#   export NOVA_GC_INCLUDE_DIR="<главная-репа>/compiler-codegen/vcpkg_installed/x64-windows-static/include"
#
# Проверено 2026-07-27: nova-polaris `nova test src --strict-effects` даёт
# PASS 35 / FAIL 0 / SKIP 16 БЕЗ локальной копии, только на этих переменных.
#
# ЧТО ПРОВЕРЯЕТ. Наличие каталога `compiler-codegen/nova_rt` в любой репе или
# worktree, КРОМЕ главной репы nova (где он и есть настоящий источник).
# Главная репа опознаётся по наличию `compiler-codegen/Cargo.toml` — там
# nova_rt законен, это его дом.
#
# ИСПОЛЬЗОВАНИЕ:
#   scripts/guards/check-no-runtime-copy.sh [каталог ...]
# Без аргументов проверяются соседние репы/worktree рядом с главной.
# Выход: 0 — чисто, 1 — найдена копия (печатает путь, размер и что делать).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Скрипт живёт в scripts/guards/ — корень репы на два уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Главная репа — та, где живёт исходник рантайма (у неё есть свой Cargo.toml
# в compiler-codegen/). Её nova_rt законен и проверкой не считается копией.
is_main_repo() {
    [ -f "$1/compiler-codegen/Cargo.toml" ]
}

targets=()
if [ "$#" -gt 0 ]; then
    targets=("$@")
else
    parent="$(cd "$REPO_ROOT/.." && pwd)"
    for d in "$parent"/*/; do
        [ -d "$d" ] || continue
        targets+=("${d%/}")
    done
fi

found=0
for t in "${targets[@]}"; do
    rt="$t/compiler-codegen/nova_rt"
    [ -d "$rt" ] || continue
    if is_main_repo "$t"; then
        continue  # дом рантайма — законно
    fi
    size="$(du -sh "$t/compiler-codegen" 2>/dev/null | cut -f1)"
    echo "НАЙДЕНА КОПИЯ РАНТАЙМА: $t/compiler-codegen ($size)" >&2
    found=1
done

if [ "$found" -ne 0 ]; then
    cat >&2 <<'HINT'

Копия рантайма внутри пакетной репы/worktree ЗАПРЕЩЕНА (реестр 221.1 №138):
она не под git, потому её протухание невидимо, и она ШАДОВИТ настоящий рантайм
— расхождение версий даёт невнятный CC-FAIL на сгенерированном коде.

Что делать:
  1) удалить каталог compiler-codegen/ из этой репы/worktree;
  2) собирать через штатные переменные, указывающие на главную репу:
       export NOVA_RT_DIR="<nova>/compiler-codegen/nova_rt"
       export NOVA_CG_INCLUDE="<nova>/compiler-codegen"
       export NOVA_STD_PATH="<nova>/std/src"
       export NOVA_GC_LIB_DIR="<nova>/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
       export NOVA_GC_INCLUDE_DIR="<nova>/compiler-codegen/vcpkg_installed/x64-windows-static/include"
HINT
    exit 1
fi

echo "check-no-runtime-copy ok: копий рантайма вне главной репы нет"
