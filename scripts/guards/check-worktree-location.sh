#!/usr/bin/env bash
# scripts/guards/check-worktree-location.sh
# Рабочие деревья (`git worktree`) живут ТОЛЬКО в `d:/Sources/nv-lang/`.
#
# ДОМ И ОСНОВАНИЕ: план 231 «Выход из цикла точечных фиксов», трек Д (машинное
# принуждение норм); запись реестра 221.1 №561; правило владельца 2026-08-10.
#
# ЗАЧЕМ — две измеренные причины, не порядок ради порядка:
#
#   1. ДИСК. Рабочее дерево Nova весит десятки мегабайт, а сборка — гигабайты.
#      Диск `C:` у нас регулярно уходит под ноль: 2026-08-10 он был занят на
#      100 % (82 МБ свободно), clang падал с `no space on device`, и это
#      выглядело как ТРИДЦАТЬ провалов тестов — полчаса ушло на поиск
#      несуществующего дефекта компилятора.
#   2. СТРАЖИ. Worktree, лежащий ВНУТРИ репозитория, попадает под все грепы:
#      2026-08-10 `check-checker-entrypoints` покраснел на шести файлах из
#      `.claude/worktrees/**` — чужих снимков на коммитах, где нужной функции
#      ещё не существовало. Страж судил наш код по чужому прошлому.
#
# ЧТО ПРОВЕРЯЕТСЯ: каждый путь из `git worktree list` лежит под
# `d:/Sources/nv-lang/` (сравнение регистронезависимое и по обеим формам
# разделителя — Windows отдаёт `D:/…`, MSYS видит `/d/…`).
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): worktree, заведённый и удалённый между
# прогонами гейта. Страж смотрит текущее состояние, а не историю.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-worktree-location.sh [КОРЕНЬ]
# ПЕРЕМЕННЫЕ:
#   NOVA_WORKTREE_ROOT — корень, под которым дозволены деревья
#                        (по умолчанию d:/Sources/nv-lang)
# Самотест — scripts/guards/selftest/test-check-worktree-location.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
ALLOWED="${NOVA_WORKTREE_ROOT:-d:/Sources/nv-lang}"

cd "$ROOT" || { echo "check-worktree-location: нет каталога $ROOT" >&2; exit 1; }
git rev-parse --git-dir >/dev/null 2>&1 || { echo "check-worktree-location ok: не git-репозиторий"; exit 0; }

# Приводим к одному виду: нижний регистр, прямые слэши, форма `/d/...` -> `d:/...`
norm() {
    printf '%s' "$1" \
        | tr 'A-Z' 'a-z' \
        | sed 's|\\|/|g' \
        | sed -E 's|^/([a-z])/|\1:/|'
}

ALLOWED_N=$(norm "$ALLOWED")
BAD=""
N=0

while IFS= read -r line; do
    case "$line" in
        worktree\ *) ;;
        *) continue ;;
    esac
    p="${line#worktree }"
    [ -n "$p" ] || continue
    N=$((N + 1))
    pn=$(norm "$p")
    case "$pn" in
        "$ALLOWED_N"/*|"$ALLOWED_N") ;;
        *) BAD="$BAD
    $p" ;;
    esac
done < <(git worktree list --porcelain 2>/dev/null)

echo "check-worktree-location: рабочих деревьев $N, дозволенный корень $ALLOWED"

if [ -n "$BAD" ]; then
    echo "check-worktree-location: НАРУШЕНИЕ — деревья вне дозволенного корня:$BAD" >&2
    echo "" >&2
    echo "    Перенеси: git worktree move <путь> $ALLOWED/<имя>" >&2
    echo "    либо сними: git worktree remove <путь>." >&2
    echo "    Причина не в порядке: диск C: уходит под ноль и ломает сборку" >&2
    echo "    чужой ошибкой, а дерево ВНУТРИ репозитория попадает под грепы" >&2
    echo "    стражей и краснит их на чужих снимках (реестр 221.1 №561)." >&2
    echo "check-worktree-location: FAIL" >&2
    exit 1
fi

echo "check-worktree-location ok: все рабочие деревья в $ALLOWED"
exit 0
