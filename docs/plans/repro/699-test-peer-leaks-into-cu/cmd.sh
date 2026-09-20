#!/bin/sh
# Реестр 221.1 №699 — пере-снятие носителя (2026-09-20).
#
# ПРЕДМЕТ строки: тестовый peer-файл модуля std объявляет `type Node`
# (`std/src/encoding/serde/tagging_test.nv:49`), а `include_test_peers` —
# ГЛОБАЛЬНЫЙ флаг, истинный для всех модулей в режиме `nova test`. Вопрос: что
# видит посторонний потребитель `encoding.serde`, объявивший СВОЙ `Node`.
#
# ТРИ РЕЖИМА — ТРИ ОТВЕТА, и разные ответы здесь и есть находка. Поэтому ни один
# режим не пропускается, даже если предыдущий уже «показал главное».
#
# Запуск из корня рабочего дерева:
#   sh docs/plans/repro/699-test-peer-leaks-into-cu/cmd.sh
set -u
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
# Бинарь можно назвать снаружи: `NOVA=<путь> sh cmd.sh`. Это не удобство —
# у окна без своего `target/` иного способа нет, а собирать его ради пробы
# дороже самой пробы и занимает машину, которая может быть под чужим гейтом.
# ВЕРСИЯ БИНАРЯ ПЕЧАТАЕТСЯ НИЖЕ: два замера разными бинарями несравнимы.
NOVA="${NOVA:-$R/nova-cli/target/release/nova.exe}"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"
[ -x "$NOVA" ] || { echo "нет собранного nova: $NOVA" >&2; exit 2; }
W="${TMPDIR:-/tmp}/repro699.$$"

rm -rf "$W"; mkdir -p "$W/src"
cp "$D/pkg/nova.toml.txt" "$W/nova.toml"
cp "$D/pkg/main.nv.txt"   "$W/src/main.nv"

echo "nova: $NOVA"
ls -l "$NOVA" 2>/dev/null | awk '{print "  размер", $5, "время", $6, $7}'
echo "носитель в дереве:"
grep -n "^type Node enum" "$R/std/src/encoding/serde/tagging_test.nv" \
    || echo "  НОСИТЕЛЯ НЕТ — предмет испарился, дальше мерить нечего"

verdict() { # $1 метка, $2.. команда
    label="$1"
    printf '%-14s ' "$1"; shift
    "$@" > "$W/o.txt" 2>&1
    rc=$?
    # Вердикт берётся из ВЫВОДА, а не из кода возврата: код возврата у разных
    # режимов значит разное, а нам нужен один язык для сравнения трёх.
    if grep -q "INTERNAL-PANIC" "$W/o.txt"; then
        echo "[INTERNAL-PANIC] rc=$rc  << строка требует, чтобы этого не было"
    elif grep -qiE "collide|collision|столкнов" "$W/o.txt"; then
        echo "КОЛЛИЗИЯ ИМЁН rc=$rc: $(grep -iE 'collide|collision|столкнов' "$W/o.txt" | head -1 | cut -c1-110)"
    elif grep -qE '\[E[0-9_A-Z]+\]|error' "$W/o.txt"; then
        echo "ОТКАЗ rc=$rc: $(grep -oE '\[E[0-9_A-Z]+\][^|]{0,90}' "$W/o.txt" | head -1)"
    elif [ "$rc" -eq 0 ]; then
        echo "ЧИСТО rc=0: $(tail -1 "$W/o.txt" | cut -c1-90)"
    else
        echo "ИНОЕ rc=$rc: $(tail -2 "$W/o.txt" | tr '\n' ' ' | cut -c1-110)"
    fi
    cp "$W/o.txt" "$W/out-$label.txt" 2>/dev/null || true
}

echo
echo "== три режима на ОДНОМ дереве и ОДНОМ бинаре =="
verdict "check" "$NOVA" check "$W/src"
verdict "test"  "$NOVA" test  "$W/src"
verdict "build" "$NOVA" build "$W/src/main.nv" -o "$W/a.exe"

echo
echo "== КОНТРОЛЬ-1: имя ИМПОРТИРОВАННОГО типа (SerError) объявлено в файле =="
cp "$D/pkg/main_control.nv.txt" "$W/src/main.nv"
verdict "control-1" "$NOVA" check "$W/src"
echo "   ожидание: это ЗАКОННО (объявление в файле старше импорта), и контроль"
echo "   слабый — он не доказывает, что путь столкновений жив."

echo
echo "== КОНТРОЛЬ-2: ДВА peer-файла одного модуля объявляют Node =="
cp "$D/pkg/main.nv.txt" "$W/src/main.nv"
cp "$D/pkg/peer_node.nv.txt" "$W/src/peer_node.nv"
verdict "control-2" "$NOVA" check "$W/src"
echo "   ЧИСТО здесь означало бы, что проба слепа и вывод выше недействителен."

echo
echo "== КОНТРОЛЬ-3: тот же peer-файл, названный ПРЯМО =="
echo "   (нужен, чтобы отличить «файлы не читались» от «читались и промолчали»)"
verdict "control-3" "$NOVA" check "$W/src/peer_node.nv"
echo "   если в выводе стоит предупреждение о ДРУГОМ файле модуля — значит peer'ы"
echo "   грузятся ВМЕСТЕ, и молчание про два `Node` есть ответ, а не слепота."
rm -f "$W/src/peer_node.nv"

echo
echo "сырые выводы: $W"
