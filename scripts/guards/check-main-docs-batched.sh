#!/usr/bin/env bash
# scripts/guards/check-main-docs-batched.sh — одиночный «бумажный» коммит в main запрещён.
#
# СЛОВО ВЛАДЕЛЬЦА 2026-10-01: «9 из 10 коммитов — доки и обвязка, мы не фиксим
# баги». Замер интегратора за двое суток: 302 коммита, из них 60 — строка
# реестра отдельным коммитом (почти все — интегратор), 34 — `docs(controller):
# log`, 28 — записки. История, в которой работа тонет в бухгалтерии, не
# показывает ни владельцу, ни CI, что сделано; каждый такой коммит — ещё и
# отдельный кандидат на прогон.
#
# ЧТО ПРОВЕРЯЕТ (зовётся из scripts/githooks/pre-commit на КАЖДЫЙ коммит):
# отказ, если ВСЁ сразу —
#   * дерево ГЛАВНОЕ, не worktree (`--git-dir` == `--git-common-dir`);
#   * ветка — main;
#   * слияние НЕ идёт (нет MERGE_HEAD: бумага, едущая в коммите слияния, —
#     законный путь);
#   * ВСЕ пути индекса (удаления тоже) лежат в «бумажном» наборе:
#       docs/plans/221.1-bug-sweep.md
#       scripts/guards/registry-rows.baseline
#       docs/dev/prompts/*-handoff.md
#       docs/dev/prompts/controller-*.md
#     (`*` — внутри каталога prompts, без подкаталогов).
# Коммит, где рядом с бумагой есть хоть один другой путь (код, фикстура,
# спека), страж не судит: он про ОДИНОЧНЫЕ бумажные коммиты, а строка реестра
# вместе со своей починкой — ровно то, чего от коммита и ждут.
#
# КАК НАДО: такие правки едут в коммите слияния или ОДНИМ пакетным коммитом на
# выкладку. Пакетный коммит — ключом NOVA_DOCS_BATCH="<что в пакете>" (текст
# обязателен и печатается: пакет без описи неотличим от одиночного).
#
# ИСПОЛЬЗОВАНИЕ: bash scripts/guards/check-main-docs-batched.sh [КОРЕНЬ]
# Самотест: scripts/guards/selftest/test-check-main-docs-batched.sh

set -u
export LC_ALL=C
NAME=check-main-docs-batched
pass() { echo "$NAME ok: $1"; exit 0; }   # страж доказывает шаг строкой ok: (№645)

ROOT="${1:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -n "$ROOT" ] && cd "$ROOT" 2>/dev/null || { echo "$NAME: не git-дерево: ${ROOT:-<пусто>}" >&2; exit 1; }

# Один вызов git на три вопроса (хук стоит на КАЖДОМ коммите).
{ read -r GD; read -r GCD; read -r BR; } <<EOF
$(git rev-parse --path-format=absolute --git-dir --git-common-dir --symbolic-full-name HEAD 2>/dev/null)
EOF
[ -n "${GD:-}" ] && [ -n "${GCD:-}" ] || { echo "$NAME: git не назвал каталоги репозитория" >&2; exit 1; }
[ "$GD" -ef "$GCD" ] || pass "worktree — не главное дерево, не судится"
[ "${BR:-}" = "refs/heads/main" ] || pass "ветка ${BR#refs/heads/} — не main, не судится"
[ -f "$GD/MERGE_HEAD" ] && pass "идёт слияние (MERGE_HEAD) — бумага в коммите слияния законна"

PATHS=$(git diff --cached --name-only --no-renames 2>/dev/null)
[ -n "$PATHS" ] || pass "индекс пуст"

N=0
while IFS= read -r p; do
    [ -n "$p" ] || continue
    N=$((N + 1))
    case "$p" in
        docs/plans/221.1-bug-sweep.md|scripts/guards/registry-rows.baseline) ;;
        docs/dev/prompts/*/*) pass "в коммите есть путь вне бумажного набора ($p)" ;;
        docs/dev/prompts/*-handoff.md|docs/dev/prompts/controller-*.md) ;;
        *) pass "в коммите есть путь вне бумажного набора ($p)" ;;
    esac
done <<EOF
$PATHS
EOF

if [ -n "${NOVA_DOCS_BATCH:-}" ]; then
    # №1479: «текст обязателен — пакет без описи неотличим от одиночного».
    # Пробелы и голое число (`1`, как у соседнего ключа NOVA_MAIN_DIRECT_CODE)
    # описью не являются — прежде проходили.
    _inv=$(printf '%s' "$NOVA_DOCS_BATCH" | tr -d ' \t')
    case "$_inv" in
        ''|*[!0-9]*) ;;
        *) _inv="" ;;
    esac
    if [ -z "$_inv" ]; then
        echo "$NAME: FAIL — NOVA_DOCS_BATCH без описи ('$NOVA_DOCS_BATCH'): назови, что в пакете" >&2
        exit 1
    fi
    pass "пакетный бумажный коммит в main ($N путей) по ключу NOVA_DOCS_BATCH: $NOVA_DOCS_BATCH"
fi

{
    echo "$NAME: FAIL — одиночный бумажный коммит в main ($N путей: только реестр/база/записки/лог контролёра):"
    printf '%s\n' "$PATHS" | head -5 | sed 's/^/    /'
    echo "  Слово владельца 2026-10-01: «9 из 10 коммитов — доки и обвязка». Такие правки"
    echo "  едут в коммите слияния или одним пакетным коммитом на выкладку:"
    echo "    NOVA_DOCS_BATCH=\"<что в пакете>\" git commit ..."
} >&2
exit 1
