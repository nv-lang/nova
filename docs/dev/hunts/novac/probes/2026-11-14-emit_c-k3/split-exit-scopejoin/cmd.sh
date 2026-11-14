#!/usr/bin/env bash
# split-exit-scopejoin (emit_c x K3, hunt 2026-11-14-emit_c-k3).
#
# Вопрос, на который два места отвечали по-разному:
#   "каким C-текстом печатается выход из supervised-области на каждом пути?"
# До фикса (свидетельство охоты, 2026-11-14): чекер отказывал по имени только
# RetStmt/LoopCtl, а `x?` и `X ?? return R` из split-тела проходили check и
# выпускали C со второй, необъявленной записью протокола
# (`use of undeclared identifier '_novac_qdl1'`, clang CC-FAIL).
# После фикса (тот же merge, интегратор): ОБЕ coalesce-двери отказывают тем же
# SUPERVISED_SPLIT_RETURN_MSG при @sup_split_depth > 0 (try_rules.nv,
# @type_try и @type_coalesce_return); bare-форма не задета (split-книга не
# открывается). Эта проба пинит ФИКС: оба носителя отказаны по имени, C не
# эмитится.
# Запуск: cmd.sh <корень>; exit 0 = отказ на месте у обоих носителей.
set -u
root="${1:-.}"
P="$(cd "$(dirname "$0")" && pwd)"
NOVAC="$root/novac/target/novac.exe"
[ -f "$NOVAC" ] || { echo "NO $NOVAC (собери novac)"; exit 2; }
export NOVA_STD_PATH="$root/std/src"
T="${TMPDIR:-/tmp}/hunt-split-exit.$$"
mkdir -p "$T"; trap 'rm -rf "$T"' 0
fail=0
for c in coalesce_return qmark; do
    echo "== $c =="
    "$NOVAC" check "$P/$c.nv" >"$T/$c.chk" 2>&1
    rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "ANCHOR LOST: check принял $c (отказ ушёл)"; cat "$T/$c.chk"; fail=1; continue
    fi
    grep -q 'return. inside the body of a .supervised' "$T/$c.chk"         && echo "witness: named refusal fires for $c"         || { echo "ANCHOR LOST: rc=$rc, но сообщение не то"; cat "$T/$c.chk"; fail=1; }
done
[ "$fail" -eq 0 ] && echo "PROBE OK: обе coalesce-записи выхода отказаны по имени" || echo "PROBE RED"
exit "$fail"
