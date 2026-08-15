#!/bin/sh
# scripts/guards/check-novac-no-global-state.sh — фазы novac не делят
# изменяемое состояние.
#
# ПРАВИЛО (план 274 §4 п.5; страж назван в §10.3): если фазы правят общий
# контекст, переиспользовать нельзя ничего — принимается сразу или не
# достигается никогда. Изменяемым состоянием прохода владеет драйвер
# (main + pipeline); фаза получает значения и возвращает значения.
#
# ⚖ ПРАВИЛО СУДИТСЯ ПРИЁМКОЙ. Целиком грепом оно не проверяемо: «общий
# изменяемый контекст» в Nova выглядит как обычная структура, протянутая
# через сигнатуры фаз, и отличить контекст от локального аккумулятора может
# только чтение. Страж проверяет ЕДИНСТВЕННОЕ машинное следствие правила —
# см. ПРОВЕРЯЕТ п.1 — и не притворяется, что закрывает правило.
#
# ПРОВЕРЯЕТ:
#  1. (работающая часть) Изменяемый АГРЕГАТ не протянут через две фазы:
#     собирает mut-параметры функций вида `fn f(... mut x TypeName ...)`,
#     где TypeName — тип, ОБЪЯВЛЕННЫЙ в самом novac/src (список типов —
#     из данных, `type X` / `export type X`, не зашит в страже); модулем
#     считается объявление `module ...` в файле, main и pipeline
#     склеиваются в один владелец «driver». Если один такой тип стоит
#     mut-параметром в двух и более модулях — красный: это и есть контекст,
#     который правят разные фазы.
#  2. (дешёвая страховка) Подстрока `static mut` и top-level mut-биндинг
#     (`mut `/`export mut ` с колонки 0). Таких форм в Nova СЕГОДНЯ НЕТ —
#     это не работающая проверка, а капкан на заимствование из Rust при
#     будущем расширении языка; числить её работающей нельзя (дефект F11,
#     честная формулировка — 2026-08-15). Имя, совпавшее со строкой
#     novac/GLOBALS.allow (одно имя на строку; пустые и '#'-строки
#     игнорируются; при override-скане файл ищется рядом со сканируемой
#     директорией), — зелёное write-once исключение.
#
# НЕ ПРОВЕРЯЕТ: mut внутри fn-тел (локальная изменяемость законна);
# mut-параметры-СТОКИ — `[]T` (вектор-аккумулятор) и типы, не объявленные в
# novac/src (StringBuilder и прочий std): сток вывода — не контекст фазы;
# mut-получателей `fn T mut @m()` — это метод типа на себе, а не протаскивание
# состояния; сигнатуры, разорванные на несколько строк (греп судит строку
# объявления); тип, честно живущий mut внутри ОДНОГО модуля (это не «между
# фазами»); протаскивание mut-агрегата внутри пары main+pipeline (драйверу
# состояние прохода держать можно); write-once-ность исключений из
# GLOBALS.allow (заявка — на совести приёмки). Нет novac/src или нет
# .nv-файлов — зелёный «судить нечего»: страж до кода легален, молчание
# нелегально (№645).
#
# $1 — корень репозитория (по умолчанию — вычислить от себя);
# $2 — override сканируемой директории (для самотеста; вместо novac/src).
#
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
ALLOW="$(dirname "$SRC")/GLOBALS.allow"
NAME=check-novac-no-global-state

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC, файлов .nv: 0)"
    exit 0
fi

NFILES=$(find "$SRC" -type f -name '*.nv' | wc -l | tr -d '[:space:]')
if [ "$NFILES" -eq 0 ]; then
    echo "$NAME ok: судить нечего (в $SRC файлов .nv: 0)"
    exit 0
fi

ALLOWED=""
if [ -f "$ALLOW" ]; then
    ALLOWED=$(sed -e 's/\r$//' -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' "$ALLOW")
fi

# --- 2. cheap trap: static mut / top-level mut (no such form in Nova today) --
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
    echo "$NAME: FAIL — общее изменяемое состояние (274 §4 п.5):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Фазы не делят изменяемый контекст: состояние течёт значениями по" >&2
    echo "  рёбрам карты. Write-once исключение — имя строкой в novac/GLOBALS.allow." >&2
    exit 1
fi

# --- 1. the working check: a mutable AGGREGATE threaded through two phases ---
# types declared by novac itself (data, not a list inside the guard)
DECLARED=$(find "$SRC" -type f -name '*.nv' | sort | while IFS= read -r f; do
    sed 's|//.*$||' "$f" | sed -n 's/^[[:space:]]*\(export[[:space:]]\{1,\}\)\{0,1\}type[[:space:]]\{1,\}\([A-Z][A-Za-z0-9_]*\).*/\2/p'
done | sort -u)

# every `mut <name> <TypeName>` parameter of a fn declaration: TYPE MODULE WHERE
USES=$(find "$SRC" -type f -name '*.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    mod=$(sed -n 's/^module[[:space:]]\{1,\}\([A-Za-z0-9_.]\{1,\}\).*/\1/p' "$f" | head -n 1)
    [ -n "$mod" ] || mod=$(dirname "$rel")
    case "$mod" in
        novac|novac.pipeline|novac.main|pipeline|main|.) mod=driver ;;
    esac
    sed 's|//.*$||' "$f" | grep -nE '^[[:space:]]*(export[[:space:]]{1,})?fn[[:space:]]' | while IFS= read -r hit; do
        num=${hit%%:*}
        line=${hit#*:}
        printf '%s\n' "$line" | grep -oE 'mut [a-z_][A-Za-z0-9_]* [A-Z][A-Za-z0-9_]*' | while IFS= read -r p; do
            ty=${p##* }
            printf '%s\n' "$DECLARED" | grep -qFx "$ty" || continue
            printf '%s %s %s:%s\n' "$ty" "$mod" "$rel" "$num"
        done
    done
done)

NUSES=$(printf '%s\n' "$USES" | grep -c '[A-Za-z]')
NTYPES=$(printf '%s\n' "$USES" | cut -d' ' -f1 | grep '[A-Za-z]' | sort -u | wc -l | tr -d '[:space:]')
SHARED=$(printf '%s\n' "$USES" | grep '[A-Za-z]' | cut -d' ' -f1,2 | sort -u | cut -d' ' -f1 | uniq -d)

if [ -n "$SHARED" ]; then
    echo "$NAME: FAIL — изменяемый агрегат протянут через несколько фаз (274 §4 п.5):" >&2
    printf '%s\n' "$SHARED" | while IFS= read -r ty; do
        mods=$(printf '%s\n' "$USES" | grep "^$ty " | cut -d' ' -f2 | sort -u | tr '\n' ' ')
        echo "  $ty: mut-параметр в модулях: $mods" >&2
        printf '%s\n' "$USES" | grep "^$ty " | while IFS=' ' read -r t m w; do
            echo "      $w ($m)" >&2
        done
    done
    echo "  Состояние прохода держит драйвер (main+pipeline); фаза берёт значения" >&2
    echo "  и возвращает значения. Либо сделай параметр немутируемым, либо оставь" >&2
    echo "  агрегат внутри одного модуля." >&2
    exit 1
fi

echo "$NAME ok: файлов .nv: $NFILES, глобальных mut вне GLOBALS.allow: 0, mut-агрегатов в сигнатурах: $NUSES (типов: $NTYPES), протянутых через две фазы: 0 (⚖ остальное судит приёмка)"
exit 0
