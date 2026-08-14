#!/bin/sh
# scripts/guards/check-novac-deps.sh — таблица рёбер novac: единственный источник.
# План: docs/plans/274-novac-self-hosted-compiler.md §10.2; вход — docs/plans/274.1-novac-architecture.md §2а.
#
# ПРАВИЛО (274 §10.2, архитектура §3): граница между модулями существует, только
# если она — строка таблицы рёбер. Импорт вне таблицы — красный. Источник ОДИН:
# сама таблица §3 в docs/dev/novac-architecture.md (markdown), второй копии в
# машинном файле НЕТ намеренно — иначе они разойдутся (класс К4).
#
# Что проверяет: каждый import в novac/src/<mod>/*.nv (и main.nv) разрешён
# строкой «| `<mod>` | ... |» таблицы §3 (колонка «в» перечисляет разрешённые
# цели). Модуль вне карты — красный. Импорты std.* — вне юрисдикции (язык).
# Что НЕ проверяет: направление данных «что течёт» (семантика — ревью) и
# внутримодульные двери.
#
# $1 — корень репо; $2 — override дерева novac (самотест); $3 — override
# файла архитектуры (самотест).
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
ARCH="${3:-$ROOT/docs/dev/novac-architecture.md}"

if [ ! -f "$ARCH" ]; then
    echo "check-novac-deps: FAIL — нет $ARCH" >&2; exit 1
fi
if [ ! -d "$SRC" ]; then
    echo "check-novac-deps ok: судить нечего (novac/src ещё нет)"; exit 0
fi

# Разрешённые рёбра из таблицы §3: строки «| `из` | `в1`, `в2` | ...»
EDGES=$(awk -F'|' '
    /^## 3\./ { t=1; next } t && /^## / { t=0 }
    t && $2 ~ /`/ {
        from=$2; gsub(/[` ]/, "", from)
        to=$3; gsub(/`/, "", to)
        if (from == "из" || from == "---") next
        n=split(to, a, ",")
        for (i=1; i<=n; i++) { gsub(/ /, "", a[i]); if (a[i] != "" ) print from ":" a[i] }
    }' "$ARCH")

if [ -z "$EDGES" ]; then
    echo "check-novac-deps: FAIL — таблица §3 не распарсилась из $ARCH" >&2; exit 1
fi

BAD=""
N=0
for f in $(find "$SRC" -name '*.nv' 2>/dev/null); do
    rel="${f#$SRC/}"
    case "$rel" in
        */*) mod="${rel%%/*}" ;;
        *)   mod="main" ;;
    esac
    # модуль обязан быть известен карте (как источник или цель)
    if ! printf '%s\n' "$EDGES" | grep -q "^$mod:\|:$mod$"; then
        if [ "$mod" != "main" ]; then
            BAD="$BAD\n  $rel: модуль '$mod' отсутствует в карте §3"
            continue
        fi
    fi
    for imp in $(grep -E '^import \.\./' "$f" 2>/dev/null | sed 's|^import \.\./||; s/[ .{].*//'); do
        N=$((N+1))
        printf '%s\n' "$EDGES" | grep -q "^$mod:$imp$" || \
            BAD="$BAD\n  $rel: импорт '$imp' — ребра '$mod -> $imp' нет в таблице §3"
    done
done

if [ -n "$BAD" ]; then
    echo "check-novac-deps: FAIL — импорты вне таблицы рёбер (архитектура §3):" >&2
    printf '%b\n' "$BAD" >&2
    echo "  Ребро добавляется ТОЛЬКО строкой таблицы §3 с контрактом «что течёт»." >&2
    exit 1
fi
E=$(printf '%s\n' "$EDGES" | wc -l)
echo "check-novac-deps ok: рёбер в карте $E, импортов проверено $N, вне таблицы 0"
exit 0
