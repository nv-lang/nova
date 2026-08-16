# scripts/dev-env.sh — КАНОНИЧЕСКИЙ рецепт сборки Nova в worktree.
#
# ЗАЧЕМ: интегратор весь день 2026-08-07 раздавал окнам НЕВЕРНЫЕ переменные
# (`NOVA_GC_INCLUDE_DIR` вместо `NOVA_INCLUDE_DIR`, путь без `compiler-codegen/`),
# и окна жгли лимиты, воюя с ошибкой `C1083: gc.h: No such file`. Одна опечатка,
# скопированная в десяток брифов. Лечение: единый источник, который окна
# `source`-ят, а НЕ копипастят из брифа.
#
# ИСПОЛЬЗОВАНИЕ (в брифе окна — одной строкой):
#   source "$(git -C d:/Sources/nv-lang/nova rev-parse --show-toplevel)/scripts/dev-env.sh"
#   cargo build --release --manifest-path nova-cli/Cargo.toml
#
# Та же деривация, что в scripts/gate.sh (там — единственный авторитет).

# Корень main-репы — ВЫВОДИТСЯ, не пишется: worktree делят один .git, и
# `git rev-parse --git-common-dir` из любого worktree указывает на main
# (`.git` — если мы и есть main). До 2026-08-16 здесь стоял путь к машине
# владельца, «сверенный с gate.sh» — то есть скопированный: класс №698.
_here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_common="$(git -C "$_here" rev-parse --git-common-dir 2>/dev/null)"
case "$_common" in
    .git|"") _NOVA_MAIN="$_here" ;;
    *)        _NOVA_MAIN="$(dirname "$_common")" ;;
esac
unset _here _common

export NOVA_GC_LIB_DIR="${_NOVA_MAIN}/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
export NOVA_INCLUDE_DIR="${_NOVA_MAIN}/compiler-codegen/vcpkg_installed/x64-windows-static/include"

# Самопроверка: если gc.h не на месте — сказать сразу, а не ловить C1083 в
# середине сборки после трёх минут ожидания.
if [ ! -f "${NOVA_INCLUDE_DIR}/gc.h" ]; then
    echo "dev-env: WARNING — gc.h не найден по ${NOVA_INCLUDE_DIR}/gc.h" >&2
    echo "dev-env: сборка рантайма упадёт C1083. Проверь путь vcpkg_installed." >&2
fi

# ЧАСТАЯ ОШИБКА, которую здесь НЕ исправить, но о которой напомнить:
#   - НЕ `NOVA_GC_INCLUDE_DIR` (такой переменной нет);
#   - путь идёт ЧЕРЕЗ compiler-codegen/, не через корневой vcpkg_installed.
