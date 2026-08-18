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
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
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

# --- ОДИН проход по всем файлам (2026-08-18) --------------------------------
# Прежняя редакция: три find|while по всем файлам, и во внутренних циклах ещё по
# процессу на КАЖДОЕ совпадение (sed, grep, printf). 27.7 секунды стены на 27
# файлах, из которых работой не было ничего. Правила ниже те же; доказательство
# — самотест и сравнение вывода на живом дереве.
#
# Проход собирает разом: (а) глобальные `mut` вне GLOBALS.allow, (б) типы,
# объявленные самим novac, (в) mut-параметры этих типов в сигнатурах.
ALLOWED_LIST=$(printf '%s\n' "$ALLOWED" | tr '\n' '|')
SCAN=$(find "$SRC" -type f -name '*.nv' | sort | xargs awk -v SRC="$SRC" -v ALLOWED="$ALLOWED_LIST" '
    function is_allowed(n,   i, a, k) { k = split(ALLOWED, a, /\|/); for (i = 1; i <= k; i++) if (a[i] == n) return 1; return 0 }

    FNR == 1 {
        rel = FILENAME; sub("^" SRC "/", "", rel)
        mod = ""
    }
    mod == "" && /^module[[:space:]]+/ {
        mod = $2; sub(/[^A-Za-z0-9_.].*$/, "", mod)
    }
    {
        raw = $0; sub(/\r$/, "", raw)
        # (а) глобальное изменяемое состояние
        if (raw ~ /^(export )?mut / || raw ~ /static mut/) {
            name = ""
            if (match(raw, /^export mut[[:space:]]+[A-Za-z_][A-Za-z0-9_]*/)) {
                name = substr(raw, RSTART, RLENGTH); sub(/^export mut[[:space:]]+/, "", name)
            } else if (match(raw, /^mut[[:space:]]+[A-Za-z_][A-Za-z0-9_]*/)) {
                name = substr(raw, RSTART, RLENGTH); sub(/^mut[[:space:]]+/, "", name)
            } else if (match(raw, /static[[:space:]]+mut[[:space:]]+[A-Za-z_][A-Za-z0-9_]*/)) {
                name = substr(raw, RSTART, RLENGTH); sub(/.*mut[[:space:]]+/, "", name)
            }
            if (name == "" || !is_allowed(name)) printf "BAD %s:%d: %s\n", rel, FNR, raw
        }
        line = raw; sub(/\/\/.*$/, "", line)
        # (б) типы, объявленные самим novac
        if (match(line, /^[[:space:]]*(export[[:space:]]+)?type[[:space:]]+[A-Z][A-Za-z0-9_]*/)) {
            t = substr(line, RSTART, RLENGTH); sub(/.*type[[:space:]]+/, "", t)
            printf "TYPE %s\n", t
        }
        # (в) mut-параметры в сигнатурах
        if (line ~ /^[[:space:]]*(export[[:space:]]+)?fn[[:space:]]/) {
            rest = line
            while (match(rest, /mut [a-z_][A-Za-z0-9_]* [A-Z][A-Za-z0-9_]*/)) {
                pair = substr(rest, RSTART, RLENGTH)
                ty = pair; sub(/.* /, "", ty)
                m = mod
                if (m == "") { m = rel; sub(/\/[^\/]*$/, "", m) }
                if (m == "novac" || m == "novac.pipeline" || m == "novac.main" ||
                    m == "pipeline" || m == "main" || m == ".") m = "driver"
                printf "USE %s %s %s:%d\n", ty, m, rel, FNR
                rest = substr(rest, RSTART + RLENGTH)
            }
        }
    }
')

BAD=$(printf '%s\n' "$SCAN" | sed -n 's/^BAD //p')
DECLARED=$(printf '%s\n' "$SCAN" | sed -n 's/^TYPE //p' | sort -u)
USES=$(printf '%s\n' "$SCAN" | sed -n 's/^USE //p' | while IFS=' ' read -r ty mod where; do
    printf '%s\n' "$DECLARED" | grep -qFx "$ty" || continue
    printf '%s %s %s\n' "$ty" "$mod" "$where"
done)
BAD=$(printf '%s\n' "$BAD" | sed 's/^/  /' | grep '[A-Za-z]')

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — общее изменяемое состояние (274 §4 п.5):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Фазы не делят изменяемый контекст: состояние течёт значениями по" >&2
    echo "  рёбрам карты. Write-once исключение — имя строкой в novac/GLOBALS.allow." >&2
    exit 1
fi

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
