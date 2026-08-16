#!/bin/sh
# scripts/guards/check-novac-cli-surface.sh — командная поверхность novac есть
# ПОДМНОЖЕСТВО поверхности nova-cli (конвенция П26; владелец 2026-08-16:
# «в main.nv ты выдумываешь свои команды, а должен брать команды nova-cli»).
#
# ЗАЧЕМ. novac делается заменой nova-cli, а не соседом: пользователь однажды
# перестанет замечать, какой бинарь у него под рукой. Всякая выдуманная
# команда — это работа по её последующему сносу плюс расхождение в привычках
# и в чужих скриптах. Проверено 2026-08-16: novac знал `check` (есть у
# nova-cli) и `emit` (у nova-cli такой команды НЕТ).
#
# ПРОВЕРЯЕТ:
#   * список команд novac — строки сравнения `a[1] == "<имя>"` /
#     `a[1] != "<имя>"` в novac/src/main.nv;
#   * список команд nova-cli — из вывода `nova --help`, раздел `Commands:`;
#   * каждая команда novac обязана быть у nova-cli, ЛИБО стоять в списке
#     осознанных расхождений `novac/cli-divergences.allow` с причиной в
#     той же строке после `#`.
# НЕ ПРОВЕРЯЕТ: совпадение флагов и текстов помощи (отдельная задача,
#   названа в П26); семантику одноимённых команд (приёмка и дифф-корпус);
#   обратное включение — novac НЕ обязан уметь всё, что умеет nova-cli.
#
# $1 — корень репозитория; $2 — override пути к main.nv; $3 — override файла
# со списком команд nova-cli (для самотеста; по умолчанию берётся из бинаря).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
MAIN="${2:-$ROOT/novac/src/main.nv}"
NAME=check-novac-cli-surface
ALLOW="$ROOT/novac/cli-divergences.allow"

if [ ! -f "$MAIN" ]; then
    echo "$NAME ok: судить нечего (нет $MAIN)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-cli-surface.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

# --- команды novac ---------------------------------------------------------
tr -d '\r' < "$MAIN" | grep -oE 'a\[1\][[:space:]]*[!=]=[[:space:]]*"[a-z][a-z-]*"' \
    | grep -oE '"[a-z][a-z-]*"' | tr -d '"' | sort -u > "$T/novac"

# --- команды nova-cli ------------------------------------------------------
if [ -n "$3" ] && [ -f "$3" ]; then
    sort -u < "$3" > "$T/cli"
else
    CLI="$ROOT/nova-cli/target/release/nova.exe"
    [ -f "$CLI" ] || CLI="$ROOT/nova-cli/target/release/nova"
    if [ ! -f "$CLI" ]; then
        MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
        [ -n "$MAINROOT" ] && CLI="$MAINROOT/../nova-cli/target/release/nova.exe"
    fi
    if [ ! -f "$CLI" ]; then
        echo "$NAME ok: судить нечего (бинаря nova-cli нет, сверять не с чем)"
        exit 0
    fi
    "$CLI" --help 2>&1 | awk '/^Commands:/{f=1;next} /^Options:/{f=0} f && /^[[:space:]]+[a-z]/ {print $1}' \
        | sort -u > "$T/cli"
fi

NCLI=$(wc -l < "$T/cli" | tr -d '[:space:]')
if [ "$NCLI" -eq 0 ]; then
    echo "$NAME: FAIL — список команд nova-cli пуст: разбор --help сломался, а молчать нельзя (класс №519)" >&2
    exit 1
fi

# --- разрешённые расхождения ----------------------------------------------
if [ -f "$ALLOW" ]; then
    sed 's/#.*//' "$ALLOW" | tr -d '\r' | sed 's/[[:space:]]*$//' | awk 'NF' | sort -u > "$T/allow"
else
    : > "$T/allow"
fi

EXTRA=$(comm -23 "$T/novac" "$T/cli" | comm -23 - "$T/allow")
if [ -n "$EXTRA" ]; then
    echo "$NAME: FAIL — команда novac, которой нет у nova-cli (конвенция П26):" >&2
    for c in $EXTRA; do echo "  $c — нет среди команд nova-cli" >&2; done
    echo "  novac делается ЗАМЕНОЙ nova-cli, а не соседом: выдуманная команда это" >&2
    echo "  работа по её сносу плюс расхождение в привычках и чужих скриптах." >&2
    echo "  Либо переименуй под существующую команду, либо заведи строку в" >&2
    echo "  novac/cli-divergences.allow с причиной и условием схождения." >&2
    exit 1
fi

# --- флаги (П26 п.5, ужесточение 2026-08-16: владелец про `--std <dir>`) ---
# Флаги, которые novac разбирает в main.nv (сравнения a[i] == "--x"), обязаны
# быть флагами ТОЙ ЖЕ команды у nova-cli (из `nova <cmd> --help`), либо стоять
# в allow. `--std` был novac-only формой: nova-cli находит std сам
# (NOVA_STD_PATH / <repo>/std/src), и novac теперь делает так же.
tr -d '' < "$MAIN" | grep -oE 'a\[[0-9]+\][[:space:]]*[!=]=[[:space:]]*"--[a-z][a-z-]*"'     | grep -oE '"--[a-z][a-z-]*"' | tr -d '"' | sort -u > "$T/nflags"
if [ -s "$T/nflags" ]; then
    if [ -n "$3" ] && [ -f "$3" ]; then
        : > "$T/cflags"   # шов самотеста: список флагов задаётся тем же файлом с префиксом
        grep -E '^--' "$3" >> "$T/cflags" 2>/dev/null || true
    else
        : > "$T/cflags"
        for c in $(cat "$T/novac"); do
            "$CLI" "$c" --help 2>&1 | grep -oE '^\s+(-[a-z], )?--[a-z][a-z-]*' | grep -oE -- '--[a-z][a-z-]*' >> "$T/cflags"
        done
    fi
    sort -u "$T/cflags" -o "$T/cflags"
    XF=$(comm -23 "$T/nflags" "$T/cflags" | comm -23 - "$T/allow")
    if [ -n "$XF" ]; then
        echo "$NAME: FAIL — флаг novac, которого нет у nova-cli для той же команды (П26):" >&2
        for f in $XF; do echo "  $f — nova-cli такого флага не знает" >&2; done
        echo "  Форма nova-cli первична: как он находит вход (std, пути) — так и novac. Либо строка allow с причиной." >&2
        exit 1
    fi
fi
NF=$(wc -l < "$T/nflags" | tr -d '[:space:]')
NN=$(wc -l < "$T/novac" | tr -d '[:space:]')
NA=$(wc -l < "$T/allow" | tr -d '[:space:]')
echo "$NAME ok: команд novac: $NN, команд nova-cli: $NCLI, флагов novac: $NF (все у nova-cli), осознанных расхождений: $NA"
exit 0
