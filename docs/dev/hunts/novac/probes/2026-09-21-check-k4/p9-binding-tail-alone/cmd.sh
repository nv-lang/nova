#!/bin/sh
# Н3 контроль 2: та же привязка без if отвергается обоими.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p9-binding-tail-alone/probe.nv"
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check (тот же файл)"
"$NOVA" check "$P"; echo "oracle rc=$?"
