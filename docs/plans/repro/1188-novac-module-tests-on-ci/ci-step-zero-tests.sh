#!/bin/sh
# Шаг CI `novac module tests` ДОСЛОВНО, но на дереве, где тестов НЕТ.
#
# Вопрос строки 1188: отличим ли «зелёный, прогнал 0 тестов» от «зелёный,
# прогнал N». Читать код шага мало — он мог бы отказать по другой причине;
# поэтому тело цикла копируется дословно из `.github/workflows/nova-gate.yml`
# и запускается на пустом каталоге.
set -uo pipefail
W="${TMPDIR:-/tmp}/ci1188.$$"
rm -rf "$W"; mkdir -p "$W/novac/src/empty_module"
cd "$W" || exit 2

rc=0
for t in novac/src/*/*_test.nv; do
    [ -e "$t" ] || continue
    echo "== $t"
    out="$(timeout 300 ./nova-cli/target/release/nova test "$t" 2>&1)" || true
    echo "$out" | tail -20
    line="$(echo "$out" | grep -E '^PASS: [0-9]+ +FAIL: [0-9]+' | tail -1)"
    if [ -z "$line" ]; then
        echo "NET stroki verdikta u $t - schitaem krasnym"
        rc=1
        continue
    fi
    case "$line" in
        *"FAIL: 0"*) ;;
        *) echo "KRASNYY: $line"; rc=1 ;;
    esac
done
echo "ITOG shaga pri NULE testov: rc=$rc"
cd /; rm -rf "$W"
exit $rc
