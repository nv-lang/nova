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
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
BIN="${2:-$ROOT/novac/target/novac.exe}"
NAME=check-novac-diag-schema

if [ ! -f "$BIN" ]; then
    echo "$NAME ok: судить нечего (novac ещё не собирается)"
    exit 0
fi

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
req = ("id", "code", "severity", "primary", "message")
for i, d in enumerate(data):
    if not isinstance(d, dict):
        sys.exit("diag %d is not an object" % i)
    miss = [k for k in req if k not in d]
    if miss:
        sys.exit("diag %d missing fields: %s" % (i, ",".join(miss)))
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
