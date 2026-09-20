#!/bin/sh
# scripts/guards/check-novac-fixture-expect.sh — фикстура, пришпилившая свой
# ТЕКСТ отказа, получает именно его.
#
# ДОМ ПРАВИЛА: план 274.7; реестр 221.1 №1189.
#
# ПРАВИЛО: строка `// NOVAC_EXPECT <подстрока>` в фикстуре
# novac/fixtures/**/neg_*.nv — обязательство: среди диагностик severity=error
# на этой фикстуре обязана быть та, чьё message содержит подстроку. Нет такой —
# красный, и в отказе печатается, что пришло вместо.
#
# ЗАЧЕМ ОТДЕЛЬНО ОТ check-novac-no-cascade: тот судит ЧИСЛО причин (ровно одна)
# и слеп ровно к тому классу, ради которого этот заведён. Замер 2026-09-20,
# строка реестра №1170: две разные формы — СРЕЗ `p[c..]` и диапазон-значение
# `ro r = 1..3` — пришли в одну дверь, и дверь ответила за обеих словами
# первой. Тридцать один носитель в исходниках самой Карины получал текст про
# голову `for`, которой в их строках нет. Каскадный страж на этом зелен:
# причина по-прежнему ОДНА. Различие двух текстов не проверял никто — а
# «развести тексты» без механизма живёт до первой правки, которая их схлопнет.
#
# ПОЧЕМУ ПОДСТРОКА, А НЕ РАВЕНСТВО: текст отказа — не контракт, его правят;
# пришпиливается РАЗЛИЧАЮЩАЯ часть («a slice `x[lo..hi]`»), а не формулировка
# целиком, иначе страж краснеет на каждой редактуре и его снимут первым.
#
# ПОЧЕМУ SHELL, А НЕ PYTHON (замер 2026-09-20, первая редакция была на python):
# самотест подменяет novac поддельным `.sh`, а на Windows python запустить его
# не может — CreateProcess отвечает «не является приложением Win32». Все стражи,
# которые ЗАПУСКАЮТ novac, по этой причине на shell; python здесь разбирает
# JSON, но бинарь зовёт оболочка.
#
# НЕ ПРОВЕРЯЕТ: фикстуры БЕЗ маркера (их число печатается — молчание
# нелегально); ЧИСЛО диагностик (это check-novac-no-cascade); схему полей
# (check-novac-diag-schema); осмысленность подстроки — это приёмка автора.
#
# $1 — корень репозитория (default: вычислить от себя);
# $2 — override бинаря novac (для самотеста).
#
# Проверялся: Windows (Git Bash), 2026-09-20.
export LC_ALL=C
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
BIN="${2:-$ROOT/novac/target/novac.exe}"
NAME=check-novac-fixture-expect
. "$(dirname "$0")/lib/novac.sh"

novac_require_bin "$NAME" "$ROOT" "$BIN"

PYBIN=$(command -v python 2>/dev/null || command -v python3 2>/dev/null)
if [ -z "$PYBIN" ]; then
    echo "$NAME: FAIL — нет python/python3 в PATH, JSON разобрать нечем" >&2
    exit 1
fi

FIXDIR="$ROOT/novac/fixtures"
T="${TMPDIR:-/tmp}/novac-fixture-expect.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

if [ -d "$FIXDIR" ]; then
    find "$FIXDIR" -type f -name 'neg_*.nv' | sort > "$T/list"
else
    : > "$T/list"
fi

# Маркер ищется ТОЛЬКО в neg_*: обязательство про отказ, а позитивная фикстура
# наблюдает эффект, а не текст.
pinned=0
plain=0
: > "$T/pinned"
while IFS= read -r f; do
    [ -n "$f" ] || continue
    want=$(sed -n 's|^[[:space:]]*//[[:space:]]*NOVAC_EXPECT[[:space:]]\{1,\}\(.*[^[:space:]]\)[[:space:]]*$|\1|p' "$f" | head -1)
    if [ -z "$want" ]; then
        plain=$((plain+1))
    else
        pinned=$((pinned+1))
        printf '%s\t%s\n' "$f" "$want" >> "$T/pinned"
    fi
done < "$T/list"

if [ "$pinned" -eq 0 ]; then
    echo "$NAME ok: судить нечего (0 фикстур с NOVAC_EXPECT; без маркера: $plain)"
    exit 0
fi

PY='
import json, sys
# Вывод идёт В ФАЙЛ, а не в консоль, и кодировка по умолчанию тогда ANSI-
# кодовая страница Windows: кириллица в самом отказе выходила знаками вопроса.
sys.stdout.reconfigure(encoding="utf-8", errors="replace")
# Вывод novac — построчный JSON; допускается и массив одной строкой. Строка,
# которую разобрать нельзя, НЕ пропускается молча: она считается и делает
# фикстуру красной, иначе страж мерил бы "что смог прочесть", а не вывод.
want = sys.argv[2]
unread = 0
msgs = []
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    line = line.strip()
    if not line:
        continue
    try:
        d = json.loads(line)
    except ValueError:
        unread += 1
        continue
    items = d if isinstance(d, list) else [d]
    for it in items:
        if isinstance(it, dict):
            if it.get("severity") == "error":
                msgs.append(it.get("message", ""))
        else:
            unread += 1
if unread:
    print("UNREAD %d" % unread)
elif any(want in m for m in msgs):
    print("OK")
else:
    print("MISS")
    for m in msgs:
        print("    пришло:         %s" % m)
    if not msgs:
        print("    пришло:         <ни одной диагностики severity=error>")
'

bad=0
: > "$T/bad"
while IFS="$(printf '\t')" read -r f want; do
    [ -n "$f" ] || continue
    rel=${f#"$ROOT"/}
    "$BIN" check "$f" > "$T/out" 2>/dev/null </dev/null
    "$PYBIN" -c "$PY" "$T/out" "$want" > "$T/verdict" 2> "$T/pyerr"
    rc=$?
    head=$(head -1 "$T/verdict" | tr -d '\r')
    if [ "$rc" -ne 0 ]; then
        printf '  %s: вывод не разобрать (%s)\n' "$rel" "$(tr -d '\r' < "$T/pyerr")" >> "$T/bad"
        bad=$((bad+1))
    elif [ "$head" = "OK" ]; then
        :
    else
        printf '  %s\n    ждал подстроку: %s\n' "$rel" "$want" >> "$T/bad"
        case "$head" in
            UNREAD*) printf '    пришло:         <%s нечитаемых строк вывода>\n' "${head#UNREAD }" >> "$T/bad" ;;
            *) tail -n +2 "$T/verdict" | tr -d '\r' >> "$T/bad" ;;
        esac
        bad=$((bad+1))
    fi
done < "$T/pinned"

if [ "$bad" -gt 0 ]; then
    echo "$NAME: FAIL — фикстура пришпилила текст, а получила другой ($bad):" >&2
    cat "$T/bad" >&2
    echo "  Маркер — обязательство, а не комментарий: либо дверь вернула не тот" >&2
    echo "  текст, либо две формы снова слились в одну (класс №1170)." >&2
    exit 1
fi
echo "$NAME ok: фикстур с пришпиленным текстом: $pinned, все совпали; без маркера: $plain"
