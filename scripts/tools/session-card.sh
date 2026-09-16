#!/bin/sh
# scripts/tools/session-card.sh — визитка сессии: как одно окно находит другое.
#
# ЗАЧЕМ. Окна общаются через `/peers`, а `SendMessage` требует ИМЯ соседа. Имя
# вида `nova-NN` живёт до первого перезапуска: замер 2026-09-16 — сессия
# интегратора звалась `nova-83`, а после рестарта стала `nova-57`, и записанное
# в файлах имя протухло в тот же час. Рядом при этом четыре безымянных соседа:
# «напиши интегратору» без адреса — указание, по которому нельзя действовать.
#
# УСТРОЙСТВО. Визитка лежит в ОБЩЕМ каталоге `.git`, который у всех worktree
# один (`git rev-parse --git-common-dir`): её видно из любого дерева, она не
# попадает в индекс, не пачкает `git status` и не требует коммита. Проверено
# 2026-09-16: из `nova` путь `.git`, из `nova-p274` — тот же каталог по
# абсолютному пути.
#
# ДВА ИСТОЧНИКА, И НИ ОДНОГО НЕ ХВАТАЕТ ПООТДЕЛЬНОСТИ. Визитка даёт ИМЯ, но не
# знает, жива ли сессия: окно могло умереть, не стерев её. `ListAgents` знает,
# кто жив, но не знает, кто из них какую роль ведёт. Поэтому правило: имя берётся
# из визитки, ЖИЗНЬ проверяется по `ListAgents`, и имени, которого там нет,
# писать некуда.
#
# ВОЗРАСТ ПЕЧАТАЕТСЯ ВСЕГДА. Визитка суточной давности — не адрес, а след:
# читатель обязан видеть, насколько она свежа, и решать сам.
#
# usage:
#   session-card.sh write <роль> <имя-сессии> [id-сессии]
#   session-card.sh read  <роль>
#   session-card.sh list
set -u

CMD="${1:-}"
GITDIR="$(git rev-parse --git-common-dir 2>/dev/null)" || {
    echo "session-card: не git-дерево" >&2; exit 2; }
case "$GITDIR" in /*|?:*) ;; *) GITDIR="$(cd "$GITDIR" && pwd)" ;; esac

card_path() { echo "$GITDIR/nova-session-$1.card"; }

case "$CMD" in
write)
    ROLE="${2:-}"; NAME="${3:-}"; SID="${4:-неизвестен}"
    [ -n "$ROLE" ] && [ -n "$NAME" ] || {
        echo "usage: session-card.sh write <роль> <имя> [id]" >&2; exit 2; }
    P="$(card_path "$ROLE")"
    {
        echo "role=$ROLE"
        echo "name=$NAME"
        echo "session_id=$SID"
        echo "tree=$(pwd)"
        echo "branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')"
        echo "epoch=$(date +%s)"
        echo "local=$(date '+%Y-%m-%d %H:%M')"
    } > "$P"
    echo "session-card: визитка записана — $P"
    echo "  роль $ROLE, имя $NAME, дерево $(pwd)"
    ;;
read)
    ROLE="${2:-}"
    [ -n "$ROLE" ] || { echo "usage: session-card.sh read <роль>" >&2; exit 2; }
    P="$(card_path "$ROLE")"
    if [ ! -f "$P" ]; then
        echo "session-card: визитки роли '$ROLE' НЕТ ($P)" >&2
        echo "  Это не значит, что окна нет: значит, оно её не писало." >&2
        echo "  Спроси владельца, какое окно ведёт эту роль, и не гадай по именам." >&2
        exit 1
    fi
    NAME="$(grep '^name=' "$P" | head -1 | cut -d= -f2-)"
    EPOCH="$(grep '^epoch=' "$P" | head -1 | cut -d= -f2-)"
    AGE=$(( $(date +%s) - ${EPOCH:-0} ))
    cat "$P"
    echo "age_sec=$AGE"
    if [ "$AGE" -gt 43200 ]; then
        echo "session-card: ВНИМАНИЕ — визитке больше 12 часов, это след, а не адрес." >&2
    fi
    echo ""
    echo "ПРОВЕРЬ ЖИЗНЬ: имя '$NAME' обязано быть в выдаче ListAgents."
    echo "Нет его там — сессия мертва, писать некуда, визитка устарела."
    ;;
list)
    ls -1 "$GITDIR"/nova-session-*.card 2>/dev/null || echo "визиток нет"
    ;;
*)
    echo "usage: session-card.sh {write <роль> <имя> [id] | read <роль> | list}" >&2
    exit 2
    ;;
esac
