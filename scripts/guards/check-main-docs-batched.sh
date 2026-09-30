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

ROOT="${1:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -n "$ROOT" ] && cd "$ROOT" 2>/dev/null || { echo "$NAME: не git-дерево: ${ROOT:-<пусто>}" >&2; exit 1; }

# Один вызов git на три вопроса (хук стоит на КАЖДОМ коммите).
{ read -r GD; read -r GCD; read -r BR; } <<EOF
$(git rev-parse --path-format=absolute --git-dir --git-common-dir --symbolic-full-name HEAD 2>/dev/null)
EOF
[ -n "${GD:-}" ] && [ -n "${GCD:-}" ] || { echo "$NAME: git не назвал каталоги репозитория" >&2; exit 1; }
[ "$GD" -ef "$GCD" ] || exit 0                      # worktree исполнителя
[ "${BR:-}" = "refs/heads/main" ] || exit 0
[ -f "$GD/MERGE_HEAD" ] && exit 0                   # бумага в коммите слияния — законно

PATHS=$(git diff --cached --name-only --no-renames 2>/dev/null)
[ -n "$PATHS" ] || exit 0

N=0
while IFS= read -r p; do
    [ -n "$p" ] || continue
    N=$((N + 1))
    case "$p" in
        docs/plans/221.1-bug-sweep.md|scripts/guards/registry-rows.baseline) ;;
        docs/dev/prompts/*/*) exit 0 ;;             # подкаталог — не бумага из набора
        docs/dev/prompts/*-handoff.md|docs/dev/prompts/controller-*.md) ;;
        *) exit 0 ;;                                # есть не-бумага — не наш случай
    esac
done <<EOF
$PATHS
EOF

if [ -n "${NOVA_DOCS_BATCH:-}" ]; then
    echo "$NAME: пакетный бумажный коммит в main ($N путей) по ключу NOVA_DOCS_BATCH: $NOVA_DOCS_BATCH"
    exit 0
fi

{
    echo "$NAME: FAIL — одиночный бумажный коммит в main ($N путей: только реестр/база/записки/лог контролёра):"
    printf '%s\n' "$PATHS" | head -5 | sed 's/^/    /'
    echo "  Слово владельца 2026-10-01: «9 из 10 коммитов — доки и обвязка». Такие правки"
    echo "  едут в коммите слияния или одним пакетным коммитом на выкладку:"
    echo "    NOVA_DOCS_BATCH=\"<что в пакете>\" git commit ..."
} >&2
exit 1
