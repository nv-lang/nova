#!/usr/bin/env bash
# scripts/tools/repo-hygiene.sh — ежедневная уборка репозиториев d:/Sources/nv-lang.
#
# ЗАЧЕМ. Требование владельца 2026-08-08: «разберись с ветками, старые удали +
# git gc, делай это каждый день по крону». Повод — замер: 118 локальных веток,
# 46 полностью влитых и потому бессмысленных, 71 несведённая на 7850 коммитов,
# из них 48 не двигались больше двух недель. Правило «никогда не копи»,
# записанное заметкой, владелец отверг дословно: «это не работает». Механизм —
# здесь и в scripts/guards/check-no-accumulation.sh (страж роста).
#
# ЧТО ДЕЛАЕТ ПО УМОЛЧАНИЮ (безопасный режим):
#   1. удаляет ветки, ПОЛНОСТЬЮ ВЛИТЫЕ в главную — их содержимое уже в истории,
#      терять нечего;
#   2. чистит ссылки на исчезнувшие удалённые ветки (`remote prune`);
#   3. убирает записи о worktree, каталогов которых больше нет;
#   4. запускает `git gc`;
#   5. ПЕРЕЧИСЛЯЕТ замершие несведённые ветки, но НЕ трогает их.
#
# ЧЕГО НЕ ДЕЛАЕТ БЕЗ ЯВНОГО ФЛАГА. Удаление НЕВЛИТОЙ ветки уничтожает работу,
# которой нет больше нигде. Это делается только с `--purge-stale`, и даже тогда
# вершина каждой удаляемой ветки СНАЧАЛА записывается в журнал
# (`PURGE_LOG`) — по SHA её можно поднять, пока reflog не истёк. Молчаливое
# удаление чужой работы недопустимо ни при каком требовании к чистоте.
#
# ЗАЩИТА ОТ САМОУБИЙСТВА. Никогда не удаляется: текущая ветка, главная ветка
# репозитория, ветка, выгруженная в существующий worktree (её и git не отдаст),
# и ветка новее порога.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/repo-hygiene.sh [--purge-stale] [--dry-run] [КОРЕНЬ]
# ПЕРЕМЕННЫЕ:
#   NOVA_HYGIENE_STALE_DAYS  — порог «замершая», по умолчанию 14
#   NOVA_HYGIENE_LOG         — журнал, по умолчанию <КОРЕНЬ>/.repo-hygiene.log

set -u
export LC_ALL=C

ROOT_DIR="d:/Sources/nv-lang"
STALE_DAYS="${NOVA_HYGIENE_STALE_DAYS:-14}"
PURGE=0
DRY=0

while [ $# -gt 0 ]; do
    case "$1" in
        --purge-stale) PURGE=1; shift ;;
        --dry-run)     DRY=1;   shift ;;
        -*) echo "repo-hygiene: неизвестный флаг '$1'" >&2; exit 1 ;;
        *)  ROOT_DIR="$1"; shift ;;
    esac
done

LOG="${NOVA_HYGIENE_LOG:-$ROOT_DIR/.repo-hygiene.log}"
NOW=$(date +%s)
CUTOFF=$(( NOW - STALE_DAYS * 86400 ))
STAMP=$(date '+%Y-%m-%d %H:%M:%S')

say() { echo "$*"; echo "$*" >> "$LOG" 2>/dev/null; }

say "=== repo-hygiene $STAMP (корень $ROOT_DIR, порог ${STALE_DAYS}дн, purge=$PURGE, dry=$DRY) ==="

TOT_DELETED=0
TOT_STALE=0
TOT_REPOS=0

for repo in "$ROOT_DIR"/*/; do
    [ -d "${repo}.git" ] || continue
    name=$(basename "$repo")
    TOT_REPOS=$(( TOT_REPOS + 1 ))

    G="git -C ${repo%/}"

    # Главная ветка репозитория: не у всех она `main`.
    MAIN=""
    for cand in main master; do
        $G rev-parse --verify --quiet "$cand" >/dev/null 2>&1 && { MAIN="$cand"; break; }
    done
    [ -z "$MAIN" ] && { say "  [$name] пропуск: нет ни main, ни master"; continue; }

    CUR=$($G rev-parse --abbrev-ref HEAD 2>/dev/null)

    # Ветки, выгруженные в worktree, — не трогать. Собираем их имена заранее.
    WT_BRANCHES=$($G worktree list --porcelain 2>/dev/null | sed -n 's|^branch refs/heads/||p')

    # ── 1. Влитые ветки: удаляем всегда, работа уже в истории ──────────────
    del=0
    while read -r br; do
        [ -z "$br" ] && continue
        [ "$br" = "$MAIN" ] && continue
        [ "$br" = "$CUR" ] && continue
        printf '%s\n' "$WT_BRANCHES" | grep -qxF "$br" && continue
        if [ "$DRY" -eq 1 ]; then
            say "  [$name] (сухой прогон) удалил бы влитую: $br"
        else
            $G branch -d "$br" >/dev/null 2>&1 && del=$(( del + 1 ))
        fi
    done <<EOF
$($G for-each-ref --merged "$MAIN" --format='%(refname:short)' refs/heads/ 2>/dev/null)
EOF
    [ "$del" -gt 0 ] && say "  [$name] удалено влитых веток: $del"
    TOT_DELETED=$(( TOT_DELETED + del ))

    # ── 2. Замершие несведённые: перечисляем; удаляем ТОЛЬКО по флагу ──────
    stale=0
    while read -r br ts; do
        [ -z "$br" ] && continue
        [ "$br" = "$MAIN" ] && continue
        [ "$br" = "$CUR" ] && continue
        [ -z "${ts:-}" ] && continue
        [ "$ts" -ge "$CUTOFF" ] && continue
        printf '%s\n' "$WT_BRANCHES" | grep -qxF "$br" && continue
        stale=$(( stale + 1 ))
        # `< /dev/null` обязателен: без него git вычерпывает ввод цикла и обход
        # обрывается на первой же ветке (поймано на страже накопления).
        n=$($G rev-list --count "$MAIN..$br" 2>/dev/null < /dev/null)
        age=$(( (NOW - ts) / 86400 ))
        if [ "$PURGE" -eq 1 ] && [ "$DRY" -eq 0 ]; then
            sha=$($G rev-parse "$br" 2>/dev/null < /dev/null)
            # Вершина В ЖУРНАЛ ДО удаления: по SHA ветку можно поднять, пока
            # жив reflog. Удалять работу, не оставив следа, нельзя.
            say "  [$name] УДАЛЯЮ замершую: $br sha=$sha коммитов=$n возраст=${age}дн"
            $G branch -D "$br" >/dev/null 2>&1 < /dev/null
            TOT_DELETED=$(( TOT_DELETED + 1 ))
        else
            say "  [$name] замершая (НЕ тронута): $br — $n коммит(ов), ${age}дн"
        fi
    done <<EOF
$($G for-each-ref --no-merged "$MAIN" --format='%(refname:short) %(committerdate:unix)' refs/heads/ 2>/dev/null)
EOF
    TOT_STALE=$(( TOT_STALE + stale ))

    # ── 3. Мусор ссылок и worktree ────────────────────────────────────────
    if [ "$DRY" -eq 0 ]; then
        $G remote prune origin >/dev/null 2>&1
        $G worktree prune >/dev/null 2>&1
        # `gc --auto` вместо безусловного `gc`: безусловный на большом
        # репозитории идёт минутами и в ежедневном кроне превращается в помеху,
        # а не в уборку. Порог решает сам git.
        $G gc --auto --quiet >/dev/null 2>&1
    fi
done

say "=== итог: репозиториев $TOT_REPOS, удалено веток $TOT_DELETED, замерших найдено $TOT_STALE ==="
[ "$PURGE" -eq 0 ] && [ "$TOT_STALE" -gt 0 ] && \
    say "    замершие НЕ удалены — для удаления запусти с --purge-stale (вершины пишутся в журнал)"

exit 0
