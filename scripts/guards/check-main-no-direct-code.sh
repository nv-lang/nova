#!/usr/bin/env bash
# scripts/guards/check-main-no-direct-code.sh — код в main приезжает только слиянием.
#
# РЕШЕНИЕ ВЛАДЕЛЬЦА 2026-09-30: интегратор не пишет код в main сам. Код делает
# исполнитель в своей ветке (фоновый агент, помощник, окно Карины), интегратор
# его ВЛИВАЕТ. Причина та же, что у решения про CI (affa00973): окно, которое
# одновременно решает задачу и сводит чужие, делает оба дела хуже, а коммит
# кода прямо в main не проходит ни чужого взгляда, ни ветки, которую можно
# отдать CI до слияния.
#
# ЧТО ПРОВЕРЯЕТ (зовётся из scripts/githooks/pre-commit на КАЖДЫЙ коммит):
# отказ, если ВСЁ сразу —
#   * дерево ГЛАВНОЕ, не worktree (`--git-dir` == `--git-common-dir`);
#   * ветка — main;
#   * слияние НЕ идёт (нет MERGE_HEAD: коммит, завершающий слияние, и есть
#     законный путь кода в main);
#   * в индексе есть путь под compiler-codegen/src, compiler-codegen/nova_rt,
#     nova-cli/src, nova-lsp/src, novac/src или std/src (удаление — тоже правка).
# Доки, реестр, спека, стражи, фикстуры на main этот страж не судит.
#
# КЛЮЧ: NOVA_MAIN_DIRECT_CODE="<причина #NNNN>" — номер строки реестра
# обязателен, причина печатается. Голое «1» — отказ: обход без причины
# неотличим от забывчивости.
#
# ИСПОЛЬЗОВАНИЕ: bash scripts/guards/check-main-no-direct-code.sh [КОРЕНЬ]
# Самотест: scripts/guards/selftest/test-check-main-no-direct-code.sh

set -u
export LC_ALL=C
NAME=check-main-no-direct-code
pass() { echo "$NAME ok: $1"; exit 0; }   # страж доказывает шаг строкой ok: (№645)

ROOT="${1:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -n "$ROOT" ] && cd "$ROOT" 2>/dev/null || { echo "$NAME: не git-дерево: ${ROOT:-<пусто>}" >&2; exit 1; }

# Один вызов git на три вопроса: хук стоит на КАЖДОМ коммите, а git на Windows
# стоит десятые секунды на запуск.
{ read -r GD; read -r GCD; read -r BR; } <<EOF
$(git rev-parse --path-format=absolute --git-dir --git-common-dir --symbolic-full-name HEAD 2>/dev/null)
EOF
[ -n "${GD:-}" ] && [ -n "${GCD:-}" ] || { echo "$NAME: git не назвал каталоги репозитория" >&2; exit 1; }
[ "$GD" -ef "$GCD" ] || pass "worktree — не главное дерево, не судится"
[ "${BR:-}" = "refs/heads/main" ] || pass "ветка ${BR#refs/heads/} — не main, не судится"
[ -f "$GD/MERGE_HEAD" ] && pass "идёт слияние (MERGE_HEAD) — законный путь кода в main"

CODE=$(git diff --cached --name-only --no-renames 2>/dev/null | grep -E \
    '^(compiler-codegen/src|compiler-codegen/nova_rt|nova-cli/src|nova-lsp/src|novac/src|std/src)/' || true)
[ -n "$CODE" ] || pass "main главного дерева, кодовых путей в индексе нет"

N=$(printf '%s\n' "$CODE" | grep -c .)
KEY="${NOVA_MAIN_DIRECT_CODE:-}"
if [ -n "$KEY" ]; then
    if printf '%s' "$KEY" | grep -qE '#[0-9]{2,5}'; then
        pass "код прямо в main ($N путей) пропущен ключом NOVA_MAIN_DIRECT_CODE: $KEY"
    fi
    echo "$NAME: FAIL — NOVA_MAIN_DIRECT_CODE должен назвать строку реестра (#NNNN), получено: '$KEY'" >&2
    exit 1
fi

{
    echo "$NAME: FAIL — коммит кода прямо в main главного дерева ($N путей), например:"
    printf '%s\n' "$CODE" | head -5 | sed 's/^/    /'
    echo "  Код в main приезжает только СЛИЯНИЕМ ветки исполнителя (фоновый агент,"
    echo "  помощник, окно Карины) — решение владельца 2026-09-30. Сделай ветку в своём"
    echo "  worktree и влей её; осознанное исключение —"
    echo "    NOVA_MAIN_DIRECT_CODE=\"<причина #NNNN>\" git commit ..."
} >&2
exit 1
