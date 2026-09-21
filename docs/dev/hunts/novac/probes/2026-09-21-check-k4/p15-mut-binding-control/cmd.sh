#!/bin/sh
# Н6 контроль: с 'mut x' тот же вызов чист.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p15-mut-binding-control/probe.nv"
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check (тот же файл)"
"$NOVA" check "$P"; echo "oracle rc=$?"
