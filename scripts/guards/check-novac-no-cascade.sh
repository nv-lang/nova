#!/bin/sh
# scripts/guards/check-novac-no-cascade.sh — «одна ошибка на одну причину»,
# никаких каскадов.
#
# ПРАВИЛО (план 274 §6, §10.3, №636 — механизмом, не прозой): каждая фикстура
# novac/fixtures/**/neg_*.nv сеет ровно одну причину, значит novac обязан
# выдать ровно ОДИН диагностик с severity=error. Больше одного — каскад,
# красный. Ноль — фикстура не отвергнута одной причиной, тоже красный.
# Дифф-гейт каскады НЕ ловит — оракул сам каскадит (§10.3).
#
# НЕ проверяет: схему полей диагностики (это check-novac-diag-schema.sh),
# severity != error (warnings/notes не считаются), поведение на pos_*.
# Контракт вызова: '<bin> check <file>', диагностика — JSON на stdout; если
# CLI novac окажется иным — страж правится тем же коммитом, что вводит бинарь.
#
# Страж «ожидает бинарь»: пока novac/target/novac.exe не существует — зелёный
# честной строкой: страж до кода легален, молчание нелегально (№645).
#
# $1 — корень репозитория (default: вычислить от себя);
# $2 — override бинаря novac (для самотеста).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
BIN="${2:-$ROOT/novac/target/novac.exe}"
NAME=check-novac-no-cascade
. "$(dirname "$0")/lib/novac.sh"

novac_require_bin "$NAME" "$ROOT" "$BIN"

PYBIN=$(command -v python 2>/dev/null || command -v python3 2>/dev/null)
if [ -z "$PYBIN" ]; then
    echo "$NAME: FAIL — нет python/python3 в PATH, JSON проверить нечем" >&2
    exit 1
fi

FIXDIR="$ROOT/novac/fixtures"
T="${TMPDIR:-/tmp}/novac-no-cascade.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

if [ -d "$FIXDIR" ]; then
    find "$FIXDIR" -type f -name 'neg_*.nv' | sort > "$T/list"
else
    : > "$T/list"
fi
N=$(wc -l < "$T/list" | tr -d ' ')
if [ "$N" -eq 0 ]; then
    echo "$NAME ok: судить нечего (0 фикстур neg_*.nv в novac/fixtures)"
    exit 0
fi

PY='
import json, sys
p = sys.argv[1]
try:
    data = json.load(open(p, encoding="utf-8"))
except Exception as e:
    sys.exit("not valid JSON: %s" % e)
if isinstance(data, dict):
    data = [data]
if not isinstance(data, list):
    sys.exit("JSON is neither object nor array")
print(sum(1 for d in data if isinstance(d, dict) and d.get("severity") == "error"))
'

bad=0
while IFS= read -r f; do
    rel=${f#"$ROOT"/}
    "$BIN" check "$f" > "$T/out" 2>/dev/null </dev/null
    n=$("$PYBIN" -c "$PY" "$T/out" 2> "$T/pyerr")
    rc=$?
    n=$(printf '%s' "$n" | tr -d '\r\n ')
    if [ "$rc" -ne 0 ] || [ -z "$n" ]; then
        printf '  %s: вывод не разобрать (%s)\n' "$rel" "$(tr -d '\r' < "$T/pyerr")" >> "$T/bad"
        bad=$((bad+1))
    elif [ "$n" -ne 1 ]; then
        if [ "$n" -eq 0 ]; then
            printf '  %s: 0 диагностик severity=error — фикстура не отвергнута одной причиной\n' "$rel" >> "$T/bad"
        else
            printf '  %s: %s диагностик severity=error — каскад, ожидался ровно один\n' "$rel" "$n" >> "$T/bad"
        fi
        bad=$((bad+1))
    fi
done < "$T/list"

if [ "$bad" -gt 0 ]; then
    echo "$NAME: FAIL — «одна причина -> один диагностик» нарушено на $bad фикстур(ах):" >&2
    cat "$T/bad" >&2
    echo "  Чинить восстановление после ошибки в novac (план 274 §6): одна" >&2
    echo "  посеянная опечатка обязана дать ровно один severity=error;" >&2
    echo "  вторичные жалобы гасятся, не дописываются в allow." >&2
    exit 1
fi
echo "$NAME ok: фикстур $N, на каждой ровно один диагностик severity=error"
exit 0
