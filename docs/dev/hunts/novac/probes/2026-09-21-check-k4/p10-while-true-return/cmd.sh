#!/bin/sh
# Н4: 'while true { return n }' — тело не кончается значением по мнению novac.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p10-while-true-return/probe.nv"
echo "=== SUBJECT: novac check"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check (тот же файл)"
"$NOVA" check "$P"; echo "oracle rc=$?"
