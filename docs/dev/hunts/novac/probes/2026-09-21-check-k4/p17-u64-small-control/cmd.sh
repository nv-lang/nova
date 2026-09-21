#!/bin/sh
# Н1 контроль 2: `u64` в аргументе подмножеству известен (маленький литерал чист).
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p17-u64-small-control/probe.nv"
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check (тот же файл)"
"$NOVA" check "$P"; echo "oracle rc=$?"
