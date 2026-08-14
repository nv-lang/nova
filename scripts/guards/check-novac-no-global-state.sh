#!/bin/sh
# scripts/guards/check-novac-no-global-state.sh — нет общего изменяемого
# состояния между фазами novac.
#
# ПРАВИЛО (план 274 §4 п.5; страж назван в §10.3): если фазы правят общий
# контекст, переиспользовать нельзя ничего — принимается сразу или не
# достигается никогда. Глобальный mut — красный; write-once исключения
# перечислены поимённо в novac/GLOBALS.allow.
#
# Проверяет грепом по .nv-файлам novac/src:
#   * top-level mut-биндинг: строка файла начинается с 'mut ' или
#     'export mut ' (колонка 0 = вне fn-тела: тело всегда с отступом);
#   * подстроку 'static mut' в любом месте строки.
# Имя биндинга, совпавшее со строкой novac/GLOBALS.allow (одно имя на
# строку; пустые и '#'-строки игнорируются; при override-скане файл ищется
# рядом со сканируемой директорией) — зелёное.
#
# НЕ проверяет: mut внутри fn-тел (локальная изменяемость законна);
# изменяемость через разделяемые структуры без mut-биндинга (это судит
# ревью фазовых сигнатур); write-once-ность самих исключений (заявка в
# GLOBALS.allow — на совести приёмки); подстроку 'static mut' судит и в
# комментариях (строже, не мягче). Нет novac/src или нет .nv-файлов —
# зелёный «судить нечего»: страж до кода легален, молчание нелегально (№645).
#
# $1 — корень репозитория (по умолчанию — вычислить от себя);
# $2 — override сканируемой директории (для самотеста; вместо novac/src).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
ALLOW="$(dirname "$SRC")/GLOBALS.allow"

if [ ! -d "$SRC" ]; then
    echo "check-novac-no-global-state ok: судить нечего (нет $SRC, файлов .nv: 0)"
    exit 0
fi

NFILES=$(find "$SRC" -type f -name '*.nv' | wc -l | tr -d '[:space:]')
if [ "$NFILES" -eq 0 ]; then
    echo "check-novac-no-global-state ok: судить нечего (в $SRC файлов .nv: 0)"
    exit 0
fi

ALLOWED=""
if [ -f "$ALLOW" ]; then
    ALLOWED=$(sed -e 's/\r$//' -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' "$ALLOW")
fi

BAD=$(find "$SRC" -type f -name '*.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    grep -nE '^(export )?mut |static mut' "$f" | while IFS= read -r hit; do
        num=${hit%%:*}
        line=${hit#*:}
        name=$(printf '%s\n' "$line" | sed -n \
            -e 's/^export mut[[:space:]]\{1,\}\([A-Za-z_][A-Za-z0-9_]*\).*/\1/p' \
            -e 's/^mut[[:space:]]\{1,\}\([A-Za-z_][A-Za-z0-9_]*\).*/\1/p' \
            -e 's/.*static[[:space:]]\{1,\}mut[[:space:]]\{1,\}\([A-Za-z_][A-Za-z0-9_]*\).*/\1/p' | head -n 1)
        if [ -n "$name" ] && printf '%s\n' "$ALLOWED" | grep -qFx "$name"; then
            continue
        fi
        printf '  %s:%s: %s\n' "$rel" "$num" "$line"
    done
done)

if [ -n "$BAD" ]; then
    echo "check-novac-no-global-state: FAIL — общее изменяемое состояние (274 §4 п.5):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Фазы не делят изменяемый контекст: состояние течёт значениями по" >&2
    echo "  рёбрам карты. Write-once исключение — имя строкой в novac/GLOBALS.allow." >&2
    exit 1
fi

echo "check-novac-no-global-state ok: файлов .nv: $NFILES, глобальных mut вне GLOBALS.allow: 0"
exit 0
