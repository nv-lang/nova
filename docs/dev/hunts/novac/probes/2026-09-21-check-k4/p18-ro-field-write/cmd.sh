#!/bin/sh
# Н5 форма 4: запись в ПОЛЕ через ro-привязку — novac молчит, оракул отвергает.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p18-ro-field-write/probe.nv"
echo "=== SUBJECT: novac check (ожидается МОЛЧАНИЕ)"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check (тот же файл)"
"$NOVA" check "$P"; echo "oracle rc=$?"
