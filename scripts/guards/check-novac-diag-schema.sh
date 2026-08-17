#!/bin/sh
# scripts/guards/check-novac-diag-schema.sh — диагностика novac валидна против
# схемы.
#
# ПРАВИЛО (план 274 §6/§7, §10.3: «вывод диагностики валиден против схемы —
# с Э1, иначе формат прирастёт как получится»): на каждой отвергаемой фикстуре
# novac/fixtures/**/neg_*.nv stdout novac обязан быть валидным JSON (объект
# или массив объектов), и каждый диагностик несёт поля id, code, severity,
# primary, message.
#
# НЕ проверяет: содержательность полей (тексты, позиции), число диагностик
# (это check-novac-no-cascade.sh), поведение на pos_* (это дифф-гейт).
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
NAME=check-novac-diag-schema
. "$(dirname "$0")/lib/novac.sh"

novac_require_bin "$NAME" "$ROOT" "$BIN"

PYBIN=$(command -v python 2>/dev/null || command -v python3 2>/dev/null)
if [ -z "$PYBIN" ]; then
    echo "$NAME: FAIL — нет python/python3 в PATH, JSON проверить нечем" >&2
    exit 1
fi

FIXDIR="$ROOT/novac/fixtures"
T="${TMPDIR:-/tmp}/novac-diag-schema.$$"
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
import json, os, sys
p = sys.argv[1]
try:
    data = json.load(open(p, encoding="utf-8"))
except Exception as e:
    sys.exit("not valid JSON: %s" % e)
if isinstance(data, dict):
    data = [data]
if not isinstance(data, list):
    sys.exit("JSON is neither object nor array")
req = ("id", "code", "severity", "primary", "message")


# ПОЗИЦИИ — часть контракта, а не украшение (план 274 §4 п.4). До 2026-08-17
# проверялось лишь НАЛИЧИЕ поля primary: диагностика с пустым primary
# проходила, а правило «позиции обязательны» держалось на эвристике в другом
# страже, которая смотрела на ИМЯ типа возврата (`-> []Token` не содержит
# слова Span) и потому промахивалась — печатала WARN и выходила нулём.
# Здесь судится сам спан: файл назван, границы целые, начало не позже конца,
# конец не за краем файла.
def check_span(i, d):
    pr = d.get("primary")
    if not isinstance(pr, dict):
        sys.exit("diag %d: primary is not an object" % i)
    for k in ("file", "start", "end"):
        if k not in pr:
            sys.exit("diag %d: primary has no %s" % (i, k))
    if not isinstance(pr["file"], str) or not pr["file"]:
        sys.exit("diag %d: primary.file is empty - the position points nowhere" % i)
    s, e = pr["start"], pr["end"]
    if not isinstance(s, int) or not isinstance(e, int):
        sys.exit("diag %d: primary.start/end are not integers" % i)
    if s < 0 or e < s:
        sys.exit("diag %d: primary span %d..%d is impossible" % (i, s, e))
    try:
        size = os.path.getsize(pr["file"])
    except OSError:
        size = None
    if size is not None and e > size:
        sys.exit("diag %d: primary.end %d is past the end of %s (%d bytes)"
                 % (i, e, pr["file"], size))
for i, d in enumerate(data):
    if not isinstance(d, dict):
        sys.exit("diag %d is not an object" % i)
    miss = [k for k in req if k not in d]
    if miss:
        sys.exit("diag %d missing fields: %s" % (i, ",".join(miss)))
    check_span(i, d)
'

bad=0
while IFS= read -r f; do
    rel=${f#"$ROOT"/}
    "$BIN" check "$f" > "$T/out" 2>/dev/null </dev/null
    if ! "$PYBIN" -c "$PY" "$T/out" 2> "$T/pyerr"; then
        printf '  %s: %s\n' "$rel" "$(tr -d '\r' < "$T/pyerr")" >> "$T/bad"
        bad=$((bad+1))
    fi
done < "$T/list"

if [ "$bad" -gt 0 ]; then
    echo "$NAME: FAIL — диагностика вне схемы на $bad фикстур(ах):" >&2
    cat "$T/bad" >&2
    echo "  Схема (план 274 §6/§7): stdout = JSON, каждый диагностик несёт" >&2
    echo "  id, code, severity, primary, message. Чинить эмиттер диагностики" >&2
    echo "  novac, не ослаблять стража." >&2
    exit 1
fi
echo "$NAME ok: фикстур $N, диагностика валидна (id,code,severity,primary,message)"
exit 0
