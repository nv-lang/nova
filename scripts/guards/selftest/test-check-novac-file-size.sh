#!/bin/sh
# Самотест check-novac-file-size.py — оба направления (норма 254):
# ловит нарушение И не краснеет на законном.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-file-size.py"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-file-size-selftest.$$"
mkdir -p "$T/src"
fails=0

ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1. Законный: короткий .nv — зелено, и строка ok: на месте (№645).
awk 'BEGIN{for(i=0;i<10;i++)print "line"}' > "$T/src/small.nv"
python "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && ok "короткий файл проходит" || bad "короткий файл покраснел"
python "$G" "$ROOT" "$T/src" 2>/dev/null | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"

# 2. Граница законна: ровно 1000 строк — зелено.
awk 'BEGIN{for(i=0;i<1000;i++)print "line"}' > "$T/src/edge.nv"
python "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && ok "ровно 1000 строк проходит" || bad "1000 строк покраснели"

# 3. Нарушение: 1001 строка — красный.
awk 'BEGIN{for(i=0;i<1001;i++)print "line"}' > "$T/src/big.nv"
python "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && bad "1001 строка прошла" || ok "1001 строка поймана"
rm -f "$T/src/big.nv"

# 4. Законный: не-.nv файл любой длины — не судится.
awk 'BEGIN{for(i=0;i<2000;i++)print "line"}' > "$T/src/huge.c"
python "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && ok "не-.nv файл не судится" || bad "не-.nv файл покраснел"

# 5. Законный: директории нет — зелено с честной строкой «судить нечего».
python "$G" "$ROOT" "$T/nope" 2>/dev/null | grep -q 'ok: судить нечего' && ok "нет директории — судить нечего" || bad "нет директории — не зелено или молчит"

# 6. Законный: .nv нет вовсе — зелено «судить нечего», не голый ноль.
mkdir -p "$T/empty"
python "$G" "$ROOT" "$T/empty" 2>/dev/null | grep -q 'ok: судить нечего' && ok "пустая директория — судить нечего" || bad "пустая директория — не зелено или молчит"

# 7. Нарушение во вложенной папке ловится (скан рекурсивный).
mkdir -p "$T/src/deep/deeper"
awk 'BEGIN{for(i=0;i<1500;i++)print "line"}' > "$T/src/deep/deeper/nested.nv"
python "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && bad "вложенный длинный файл прошёл" || ok "вложенный длинный файл пойман"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-file-size ok: 8/8"
    exit 0
fi
echo "test-check-novac-file-size: FAIL ($fails)" >&2
exit 1
