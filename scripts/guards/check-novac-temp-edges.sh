#!/bin/sh
# scripts/guards/check-novac-temp-edges.sh — временные рёбра таблицы §3 и
# временные ice-маркеры в коде САМОИСТЕКАЮТ (аудит механизмов 2026-08-16, дыра №6: рёбра «Э1/Э2-временные»
# не имели ни храповика, ни даты, ни стража на слово «временное» — легализованы
# таблицей бессрочно; из-за них шов кодогена §2г не проверялся, а был
# разрешён навсегда).
#
# МЕХАНИЗМ. Образец самоистекания в проекте уже был — привязка строгости к
# `oracle-pin` в check-novac-type-field-docs.sh. Здесь якорь другой: ЭТАП.
# В novac/nova.toml живёт машинная строка `#   stage: <этап>` (та же дверь,
# что spec-point/oracle-pin). Каждое временное ребро таблицы §3 помечено
# `until:<этап>` — этап, ДО которого оно законно. Как только stage двигается
# на этот этап или дальше — ребро красное, пока не снято той же волной.
#
# ПРОВЕРЯЕТ:
#   * ТОТ ЖЕ якорь держит второй носитель срока — маркеры в КОДЕ вида
#     `[LEGACY-#TBD-user-error-as-ice until:<этап>]` (интегратор 2026-08-16:
#     ice на ошибке пользователя обязан жить под маркером со сроком, иначе
#     «later wave» становится навсегда): маркер без until: — красный; маркер
#     с until: <= текущего этапа — красный, чекер обязан был его забрать;
#   * строка `#   stage:` в novac/nova.toml существует и несёт известный этап
#     (порядок: E1 < E2 < E2b1 < E2b2 < E2b3 < E3 < E4 < E5 < E6);
#   * каждая строка таблицы §3 архитектуры со словом «временн» несёт пометку
#     `until:<этап>` — временное без срока запрещено (это и была дыра);
#   * пометка `until:` с этапом <= текущему — красный: ребро истекло.
# НЕ ПРОВЕРЯЕТ: что снятое ребро действительно исчезло из импортов (это
#   check-novac-deps.sh по таблице); осмысленность выбранного этапа (приёмка).
#
# $4 — override директории кода с маркерами (самотест).
# $1 — корень; $2 — override файла архитектуры; $3 — override nova.toml.
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
ARCH="${2:-$ROOT/docs/dev/novac-architecture.md}"
TOML="${3:-$ROOT/novac/nova.toml}"
NAME=check-novac-temp-edges

if [ ! -f "$ARCH" ]; then
    echo "$NAME ok: судить нечего (нет $ARCH)"
    exit 0
fi
[ -f "$TOML" ] || { echo "$NAME: FAIL — нет $TOML: якоря этапа нет, временные рёбра истечь не могут" >&2; exit 1; }

STAGE=$(tr -d '\r' < "$TOML" | sed -n 's/^#   stage: \([A-Za-z0-9]*\)$/\1/p' | head -n 1)
[ -n "$STAGE" ] || { echo "$NAME: FAIL — в $TOML нет строки '#   stage: <этап>' (строгий формат, как у spec-point): без якоря временное ребро вечно" >&2; exit 1; }

ORDER="E1 E2 E2b1 E2b2 E2b3 E3 E4 E5 E6"
rank() { i=0; for s in $ORDER; do i=$((i+1)); [ "$s" = "$1" ] && { echo $i; return; }; done; echo 0; }
CUR=$(rank "$STAGE")
[ "$CUR" -gt 0 ] || { echo "$NAME: FAIL — этап '$STAGE' неизвестен (законны: $ORDER)" >&2; exit 1; }

T="${TMPDIR:-/tmp}/novac-temp-edges.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

# строки таблицы рёбер со словом «временн» (только строки таблиц: начинаются с |)
tr -d '\r' < "$ARCH" | awk '/^\|/ && /временн/ { print NR ": " $0 }' > "$T/temp"
NT=$(wc -l < "$T/temp" | tr -d '[:space:]')

NOUNTIL=$(grep -v 'until:E[0-9b]*' "$T/temp" | cut -c1-90)
EXPIRED=""
while IFS= read -r line; do
    [ -n "$line" ] || continue
    u=$(printf '%s\n' "$line" | grep -o 'until:E[0-9b]*' | head -n 1 | sed 's/until://')
    [ -n "$u" ] || continue
    r=$(rank "$u")
    if [ "$r" -eq 0 ]; then
        EXPIRED="$EXPIRED
  $(printf '%s' "$line" | cut -c1-80) — этап '$u' неизвестен"
    elif [ "$CUR" -ge "$r" ]; then
        EXPIRED="$EXPIRED
  $(printf '%s' "$line" | cut -c1-80) — истекло: until:$u, а stage уже $STAGE"
    fi
done < "$T/temp"

if [ -n "$NOUNTIL" ]; then
    echo "$NAME: FAIL — временное ребро БЕЗ срока (274.1 §2в; аудит 2026-08-16 дыра №6):" >&2
    printf '%s\n' "$NOUNTIL" | sed 's/^/  /' >&2
    echo "  Временное обязано нести until:<этап> — иначе оно вечное с красивым словом." >&2
    exit 1
fi
if [ -n "$EXPIRED" ]; then
    echo "$NAME: FAIL — временное ребро ИСТЕКЛО:" >&2
    printf '%s\n' "$EXPIRED" >&2
    echo "  Этап наступил — сними ребро из таблицы §3 и из импортов той же волной," >&2
    echo "  либо сдвинь until: сознательным коммитом с причиной, почему шов ещё жив." >&2
    exit 1
fi

# --- маркеры в коде: [LEGACY-#...-user-error-as-ice until:<этап>] ---------
SRC="${4:-$ROOT/novac/src}"
NM=0; MBAD=""
if [ -d "$SRC" ]; then
    grep -rn "user-error-as-ice" "$SRC" --include='*.nv' 2>/dev/null | grep -v '_test\.nv' > "$T/marks" || true
    NM=$(wc -l < "$T/marks" | tr -d '[:space:]')
    while IFS= read -r m; do
        [ -n "$m" ] || continue
        loc=$(printf '%s' "$m" | cut -d: -f1-2 | sed "s|$SRC/||")
        u=$(printf '%s' "$m" | grep -o 'until:E[0-9b]*' | head -n 1 | sed 's/until://')
        if [ -z "$u" ]; then MBAD="$MBAD
  $loc — маркер user-error-as-ice БЕЗ until:<этап>"; continue; fi
        r=$(rank "$u")
        if [ "$r" -eq 0 ]; then MBAD="$MBAD
  $loc — этап '$u' неизвестен"
        elif [ "$CUR" -ge "$r" ]; then MBAD="$MBAD
  $loc — истёк: until:$u, а stage уже $STAGE — чекер обязан был забрать эту ошибку у ice"
        fi
    done < "$T/marks"
fi
if [ -n "$MBAD" ]; then
    echo "$NAME: FAIL — ice на ошибке пользователя пережил свой срок или стоит без срока:" >&2
    printf '%s
' "$MBAD" >&2
    echo "  Ошибка пользователя — диагностика, не ice (нулевая терпимость). Маркер держит" >&2
    echo "  это временно и ТОЛЬКО со сроком; наступил этап — переноси в check той же волной." >&2
    exit 1
fi
echo "$NAME ok: stage $STAGE, временных рёбер $NT, все со сроком, истёкших 0; ice-маркеров на ошибке пользователя $NM, все со сроком"
exit 0
